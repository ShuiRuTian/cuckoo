//! `cuckoo-ca`：证书体系（根 CA 生成/持久化/安装引导，叶子证书签发）。
//!
//! 基于 `spec.md` 4.6 节的设计：
//! - 应用首次启动生成根 CA（`rcgen`），私钥+证书持久化到 OS 应用数据目录。
//! - 叶子证书按域名现场签发，`DashMap<String, Arc<ServerConfig>>` 异步缓存。
//! - 有效期/扩展字段（SAN、AuthorityKeyIdentifier 等）策略由本 crate 控制。
//!
//! 使用方式：
//! ```ignore
//! let ca = CaAuthority::load_or_create().await?;
//! let server_config = ca.get_or_issue_server_config("example.com")?;
//! let ca_pem = ca.export_ca_cert_pem()?;
//! ```

mod authority;
mod error;

pub use authority::CaAuthority;
pub use error::CaError;
