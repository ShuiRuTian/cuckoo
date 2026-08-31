//! `cuckoo-proxy` 错误类型。

use std::io;

/// 代理操作错误。
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("HTTP parse error: {0}")]
    HttpParse(String),

    #[error("connection closed unexpectedly")]
    ConnectionClosed,

    #[error("connect target parse failed: {0}")]
    ConnectParseFailed(String),

    #[error("upstream connect failed: {0}")]
    UpstreamConnectFailed(String),

    #[error("handler error: {0}")]
    Handler(String),
}

pub type ProxyResult<T> = Result<T, ProxyError>;

impl From<rustls::Error> for ProxyError {
    fn from(e: rustls::Error) -> Self {
        ProxyError::Tls(e.to_string())
    }
}
