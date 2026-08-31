//! CONNECT 方法处理（`spec.md` 4.3 节，`plan.md` M2.2 节）。
//!
//! 显式代理模式：
//! 1. 解析 CONNECT 请求行（`CONNECT host:port HTTP/1.1`）
//! 2. 回复 `200 Connection Established` 建立隧道
//! 3. 隧道内 peek 前几个字节区分 TLS / 明文 HTTP / 未知协议
//! 4. TLS → `tls::handle_tls`；明文 → `http1::serve`；未知 → 透传

use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::error::{ProxyError, ProxyResult};
use crate::handler::{FlowContext, SharedHandler};
use crate::tls;

/// 解析 CONNECT 请求行，提取目标 host 和 port。
///
/// 格式：`CONNECT host:port HTTP/1.1`
pub fn parse_connect_target(connect_line: &str) -> ProxyResult<(String, u16)> {
    // CONNECT host:port HTTP/1.1
    let parts: Vec<&str> = connect_line.split_whitespace().collect();
    if parts.len() < 2 || !parts[0].eq_ignore_ascii_case("CONNECT") {
        return Err(ProxyError::ConnectParseFailed(format!(
            "not a CONNECT request: {connect_line}"
        )));
    }

    let target = parts[1];
    // host:port 或 [ipv6]:port
    if let Some(colon_pos) = target.rfind(':') {
        let host = &target[..colon_pos];
        let port_str = &target[colon_pos + 1..];
        let port: u16 = port_str.parse().map_err(|_| {
            ProxyError::ConnectParseFailed(format!("invalid port in CONNECT target: {target}"))
        })?;

        // 去掉 IPv6 地址的方括号
        let host = host.trim_start_matches('[').trim_end_matches(']');
        Ok((host.to_string(), port))
    } else {
        // 无端口，默认 443
        Ok((target.to_string(), 443))
    }
}

/// 处理 CONNECT 隧道请求。
///
/// 1. 回复 200 Connection Established
/// 2. peek 隧道内前几个字节探测协议
/// 3. 按协议分流到 TLS / HTTP / 透传
pub async fn handle_connect_tunnel(
    mut stream: TcpStream,
    connect_line: &str,
    handler: SharedHandler,
    ca: std::sync::Arc<cuckoo_ca::CaAuthority>,
) -> ProxyResult<()> {
    let (host, port) = parse_connect_target(connect_line)?;
    tracing::debug!(host = %host, port, "CONNECT tunnel established");

    // 回复 200 Connection Established
    stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await?;

    // peek 隧道内的前几个字节：循环直到凑够探测字节或超时。
    // 单次 peek 可能因 TCP 分片只返回 1-2 字节，直接判断会把
    // HTTP 请求误判为未知协议而透传（完全不进抓包/规则系统）。
    let (peek_buf, n) = peek_enough(&mut stream, 4).await?;

    if n >= 2 && peek_buf[0] == 0x16 && peek_buf[1] == 0x03 {
        // TLS record header → TLS 终止
        let ctx = FlowContext::new(&host, port);
        tls::handle_tls(stream, &host, port, handler, ca, ctx).await
    } else if n >= 3 {
        // 明文 HTTP（用已到达的前缀匹配常见方法）
        let prefix = &peek_buf[..n];
        let known_methods: [&[u8]; 7] = [b"GET", b"POST", b"PUT", b"HEAD", b"PATCH", b"DELETE", b"OPTIONS"];
        let is_http = known_methods
            .iter()
            .any(|m| prefix.len() >= m.len() && &prefix[..m.len()] == *m);
        if is_http {
            tracing::debug!(host = %host, port, "plain HTTP in CONNECT tunnel");
            let ctx = FlowContext::new(&host, port);
            crate::listener::handle_plain_http(stream, &host, port, handler, ctx).await
        } else {
            // 未知协议 → 透传
            tracing::debug!(host = %host, port, "unknown protocol, passthrough");
            passthrough_bidirectional(stream, &host, port).await
        }
    } else {
        // 超时后数据仍太少，当作透传
        passthrough_bidirectional(stream, &host, port).await
    }
}

/// 循环 peek 直到凑够 `min` 字节或超时（~2s），返回（缓冲区，已到达字节数）。
async fn peek_enough(stream: &mut TcpStream, min: usize) -> ProxyResult<([u8; 8], usize)> {
    let mut buf = [0u8; 8];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let n = stream.peek(&mut buf).await?;
        if n == 0 {
            return Err(ProxyError::ConnectionClosed);
        }
        if n >= min || tokio::time::Instant::now() >= deadline {
            return Ok((buf, n));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 双向透传：建立到目标的 TCP 连接，双向 copy 数据。
///
/// 用于不支持 TLS 终止或未知协议的场景。
pub async fn passthrough_bidirectional(
    mut client: TcpStream,
    host: &str,
    port: u16,
) -> ProxyResult<()> {
    let addr = format!("{host}:{port}");
    let mut upstream = match TcpStream::connect(&addr).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(%addr, ?e, "passthrough: upstream connect failed");
            return Err(ProxyError::UpstreamConnectFailed(format!(
                "passthrough connect {addr}: {e}"
            )));
        }
    };

    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}
