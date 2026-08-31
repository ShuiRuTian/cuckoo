//! 请求转发到真实上游服务器（`spec.md` 4.2 节，`plan.md` M2.2 节）。
//!
//! - 解析目标 host:port，建立 TCP 连接
//! - 请求序列化复用 `http1::write_request`（共享 framing 规范化逻辑：
//!   去重 Content-Length / 丢弃已解块的 Transfer-Encoding）
//! - HTTPS 上游走 tokio_rustls（验证系统信任链，TLS connector 全局缓存复用）
//! - 不支持连接复用（每个请求独立连接，M5 补齐）

use std::sync::Arc;

use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::error::{ProxyError, ProxyResult};
use crate::handler::HttpMessage;
use crate::http1;

/// 全局 TLS connector（含系统根证书），避免每个 HTTPS 请求重复加载证书库。
///
/// `load_native_certs()` 每次调用都会遍历系统信任库并新建 `ClientConfig`，
/// 高流量场景下这是显著开销；用 `OnceLock` 惰性初始化一次全局复用。
static TLS_CONNECTOR: std::sync::OnceLock<TlsConnector> = std::sync::OnceLock::new();

fn tls_connector() -> Result<&'static TlsConnector, ProxyError> {
    if let Some(c) = TLS_CONNECTOR.get() {
        return Ok(c);
    }

    let mut root_store = rustls::RootCertStore::empty();
    let native_certs = rustls_native_certs::load_native_certs()?;
    for cert in native_certs {
        root_store.add(cert).map_err(|e| {
            ProxyError::Tls(format!("failed to add native cert: {e}"))
        })?;
    }

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));
    let _ = TLS_CONNECTOR.set(connector);
    Ok(TLS_CONNECTOR.get().expect("connector just set"))
}

/// 转发请求到上游服务器并返回响应。
///
/// `host` / `port` 指定上游地址，`is_tls` 指示是否需要 TLS 连接（HTTPS）。
pub async fn forward_request(
    host: &str,
    port: u16,
    is_tls: bool,
    req: &HttpMessage,
) -> ProxyResult<HttpMessage> {
    if is_tls {
        return forward_https_request(host, port, req).await;
    }

    forward_http_request(host, port, req).await
}

/// 明文 HTTP 转发。
async fn forward_http_request(host: &str, port: u16, req: &HttpMessage) -> ProxyResult<HttpMessage> {
    let addr = format!("{host}:{port}");
    tracing::debug!(%addr, method = %req.method, uri = %req.uri, "connecting to upstream");

    let mut stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| ProxyError::UpstreamConnectFailed(format!("connect {addr}: {e}")))?;

    http1::write_request(&mut stream, req).await?;

    // 传入请求方法：HEAD 请求的响应无 body，避免解析器空等
    let response = http1::read_response(&mut stream, &req.method).await?;
    Ok(response)
}

/// HTTPS 上游转发（rustls 客户端，验证系统信任链）。
async fn forward_https_request(
    host: &str,
    port: u16,
    req: &HttpMessage,
) -> ProxyResult<HttpMessage> {
    use rustls::pki_types::ServerName;

    let addr = format!("{host}:{port}");
    tracing::debug!(%addr, method = %req.method, uri = %req.uri, "connecting to HTTPS upstream");

    let tcp_stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| ProxyError::UpstreamConnectFailed(format!("connect {addr}: {e}")))?;

    let connector = tls_connector()?;
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|e| ProxyError::Tls(format!("invalid server name '{host}': {e}")))?;

    let mut tls_stream = connector
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| ProxyError::Tls(format!("TLS handshake to {host}:{port}: {e}")))?;

    http1::write_request(&mut tls_stream, req).await?;

    let response = http1::read_response(&mut tls_stream, &req.method).await?;
    Ok(response)
}
