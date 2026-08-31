//! TLS 终止与动态证书签发（`spec.md` 4.4 节，`plan.md` M2.2 节）。
//!
//! 拦截决策流程：
//! 1. 先用 `TcpStream::peek`（**不消费**）读取 ClientHello 原始字节，
//!    手工解析出 SNI，调用 `handler.should_intercept_tls` 做拦截决策。
//! 2. 透传：peek 的字节仍留在内核缓冲区，直接对上游做 `copy_bidirectional`
//!    即可完整转发整个 TLS 流（包含 ClientHello），上游握手正常。
//! 3. 拦截：把（未被 peek 消费的）流交给 `LazyConfigAcceptor`，
//!    由 rustls 权威解析 ClientHello，接入 `cuckoo-ca` 现场签发证书并完成握手。

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio_rustls::LazyConfigAcceptor;
use tokio_rustls::server::TlsStream;

use crate::error::{ProxyError, ProxyResult};
use crate::handler::{FlowContext, SharedHandler};
use crate::listener;

/// 循环 peek 直到读到较完整的 ClientHello（或超时）。
///
/// TCP 分片到达时单次 peek 可能只有 1-2 字节，直接判断会误判协议；
/// ClientHello 通常 < 4KB，这里最多等 ~2s、凑够完整 TLS record 即返回。
async fn peek_client_hello(stream: &mut TcpStream) -> ProxyResult<Vec<u8>> {
    let mut buf = vec![0u8; 8192];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);

    loop {
        let n = stream.peek(&mut buf).await?;
        if n == 0 {
            return Err(ProxyError::ConnectionClosed);
        }

        // ClientHello record 已完整到达（record 头 5 字节含长度）
        if n >= 5 {
            let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
            if n >= 5 + record_len {
                return Ok(buf[..n].to_vec());
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Ok(buf[..n].to_vec());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 从 ClientHello 原始字节中尽力解析 SNI。
///
/// 布局：record(5) → handshake(4) → client_version(2) + random(32) +
/// session_id(1+n) + cipher_suites(2+n) + compression(1+n) + extensions(2+n)，
/// SNI 位于 extension type 0x0000。解析失败返回 `None`（按无 SNI 处理）。
fn parse_sni_from_client_hello(data: &[u8]) -> Option<String> {
    fn take<'a>(buf: &'a [u8], i: &mut usize, n: usize) -> Option<&'a [u8]> {
        if buf.len() < *i + n {
            return None;
        }
        let s = &buf[*i..*i + n];
        *i += n;
        Some(s)
    }

    // TLS record header: type(0x16) + version(2) + length(2)
    if data.first() != Some(&0x16) || data.len() < 5 {
        return None;
    }
    let record_len = u16::from_be_bytes([data[3], data[4]]) as usize;
    let body = &data[5..(5 + record_len).min(data.len())];

    // handshake header: type(0x01 ClientHello) + length(3)
    if body.first() != Some(&0x01) || body.len() < 4 {
        return None;
    }
    let hs_len = (body[1] as usize) << 16 | (body[2] as usize) << 8 | body[3] as usize;
    let ch = &body[4..(4 + hs_len).min(body.len())];

    let mut i = 0usize;
    let _version = take(ch, &mut i, 2)?;
    let _random = take(ch, &mut i, 32)?;
    let sid_len = take(ch, &mut i, 1)?[0] as usize;
    take(ch, &mut i, sid_len)?;
    let cs_len = u16::from_be_bytes(take(ch, &mut i, 2)?.try_into().ok()?) as usize;
    take(ch, &mut i, cs_len)?;
    let cm_len = take(ch, &mut i, 1)?[0] as usize;
    take(ch, &mut i, cm_len)?;

    let ext_total = u16::from_be_bytes(take(ch, &mut i, 2)?.try_into().ok()?) as usize;
    let exts = take(ch, &mut i, ext_total)?;

    let mut j = 0usize;
    while j + 4 <= exts.len() {
        let ext_type = u16::from_be_bytes([exts[j], exts[j + 1]]);
        let ext_len = u16::from_be_bytes([exts[j + 2], exts[j + 3]]) as usize;
        j += 4;
        if exts.len() < j + ext_len {
            break;
        }
        let ext = &exts[j..j + ext_len];
        if ext_type == 0x0000 {
            // server_name: list_len(2) [ type(1)=0 name_len(2) name ]
            if ext.len() >= 5 && ext[2] == 0 {
                let name_len = u16::from_be_bytes([ext[3], ext[4]]) as usize;
                if ext.len() >= 5 + name_len {
                    return Some(String::from_utf8_lossy(&ext[5..5 + name_len]).to_string());
                }
            }
        }
        j += ext_len;
    }
    None
}

/// 处理 TLS 连接：终止 TLS → 解析 HTTP → 转发。
///
/// 流程（`spec.md` 4.4 节）：
/// 1. peek ClientHello，解析 SNI
/// 2. 调用 `handler.should_intercept_tls` 决定是否 MITM
/// 3. 如果不拦截 → 透传（兼容证书锁定）
/// 4. 如果拦截 → 查/签发证书 → 完成握手 → HTTP/1.1 处理
pub async fn handle_tls(
    stream: TcpStream,
    host: &str,
    port: u16,
    handler: SharedHandler,
    ca: Arc<cuckoo_ca::CaAuthority>,
    ctx: FlowContext,
) -> ProxyResult<()> {
    // 先 peek（不消费）ClientHello，解析 SNI 供拦截决策。
    // peek 不从 socket 移除数据，后续两条路径都能读到完整字节流：
    // - 透传：copy_bidirectional 连同 ClientHello 一起转发
    // - 拦截：LazyConfigAcceptor 重新读取并权威解析
    let mut stream = stream;
    let client_hello = peek_client_hello(&mut stream).await?;
    let sni = parse_sni_from_client_hello(&client_hello);

    let ctx = ctx.with_sni(sni.clone());
    if !handler.should_intercept_tls(sni.as_deref(), &ctx) {
        // 透传：ClientHello 字节仍在内核缓冲区未被消费，
        // copy_bidirectional 会完整转发整个 TLS 流，上游握手正常。
        tracing::debug!(host = %host, sni = ?sni, "TLS passthrough (no intercept)");
        let mut upstream = TcpStream::connect(format!("{host}:{port}"))
            .await
            .map_err(|e| {
                ProxyError::UpstreamConnectFailed(format!(
                    "passthrough connect {host}:{port}: {e}"
                ))
            })?;
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
        return Ok(());
    }

    // 拦截：LazyConfigAcceptor 读取 ClientHello（字节仍在 socket 缓冲中）
    let acceptor = LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream);
    let start = acceptor
        .await
        .map_err(|e| ProxyError::Tls(format!("LazyConfigAcceptor failed: {e}")))?;

    // 用 rustls 的权威解析结果修正 SNI（peek 解析是尽力而为的）
    let sni = start.client_hello().server_name().map(String::from);
    let ctx = ctx.with_sni(sni.clone());

    // 查缓存或现场签发证书
    let domain = sni.as_deref().unwrap_or(host);
    tracing::debug!(host = %host, sni = %domain, "TLS intercept: issuing cert");

    let server_config = ca
        .get_or_issue_server_config(domain)
        .map_err(|e| ProxyError::Tls(format!("CA cert issuance failed: {e}")))?;

    // 完成握手
    let tls_stream = start
        .into_stream(server_config)
        .await
        .map_err(|e| ProxyError::Tls(format!("TLS handshake failed: {e}")))?;

    // ALPN 只声明 http/1.1（见 cuckoo-ca authority），统一走 HTTP/1.1 处理；
    // 待 http2.rs 落地后按 ALPN 结果分流。
    serve_tls_http1(tls_stream, host, port, handler, ctx).await
}

/// 在 TLS 流上处理 HTTP/1.1 请求。
async fn serve_tls_http1(
    mut tls_stream: TlsStream<TcpStream>,
    host: &str,
    port: u16,
    handler: SharedHandler,
    ctx: FlowContext,
) -> ProxyResult<()> {
    listener::handle_plain_http_on_tls(&mut tls_stream, host, port, handler, ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sni_empty() {
        assert_eq!(parse_sni_from_client_hello(&[]), None);
        assert_eq!(parse_sni_from_client_hello(&[0x16]), None);
        assert_eq!(parse_sni_from_client_hello(&[0x17, 0x03, 0x01, 0x00, 0x10]), None);
    }
}
