//! `cuckoo-ca` 错误类型。

use cuckoo_core::ServiceError;

/// 证书操作错误，可转换为 `ServiceError` 供 Service 层使用。
#[derive(Debug, thiserror::Error)]
pub enum CaError {
    #[error("failed to generate CA: {0}")]
    GenerateFailed(String),

    #[error("failed to persist CA: {0}")]
    PersistFailed(String),

    #[error("failed to load CA: {0}")]
    LoadFailed(String),

    #[error("failed to sign leaf certificate: {0}")]
    SignFailed(String),

    #[error("failed to build TLS server config: {0}")]
    TlsConfigFailed(String),

    #[error("CA not initialized")]
    NotInitialized,
}

impl From<CaError> for ServiceError {
    fn from(e: CaError) -> Self {
        ServiceError::Internal(format!("CA error: {e}"))
    }
}

impl From<rcgen::Error> for CaError {
    fn from(e: rcgen::Error) -> Self {
        CaError::GenerateFailed(e.to_string())
    }
}

impl From<std::io::Error> for CaError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => CaError::LoadFailed(e.to_string()),
            _ => CaError::PersistFailed(e.to_string()),
        }
    }
}

impl From<rustls::Error> for CaError {
    fn from(e: rustls::Error) -> Self {
        CaError::TlsConfigFailed(e.to_string())
    }
}
