//! CA 证书管理器：根 CA 生成/持久化 + 叶子证书现场签发 + 缓存。
//!
//! 设计要点（`spec.md` 4.6 节）：
//! - 根 CA 用 `rcgen` 生成，私钥（PKCS#8 DER）+ 证书（PEM）持久化到应用数据目录。
//! - 叶子证书按域名（SNI）现场签发，缓存到 `DashMap<String, Arc<ServerConfig>>`。
//! - ALPN 同时声明 `h2` 和 `http/1.1`（与 spec.md 4.4 节一致）。

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};
use rustls::server::ServerConfig;
use rustls::sign::CertifiedKey;
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer},
    RootCertStore, ServerConfig as RustlsServerConfig,
};
use tokio_rustls::TlsAcceptor;

use crate::error::CaError;

/// 根 CA 证书的持久化数据。
struct StoredCa {
    /// 根 CA 证书（DER 格式）。
    cert_der: CertificateDer<'static>,
    /// 用于签发叶子证书的 `rcgen` KeyPair。
    key_pair: KeyPair,
    /// 用于签发叶子证书的 `rcgen` CA 证书对象。
    ca_cert: rcgen::Certificate,
}

/// CA 证书管理器。
///
/// 维护根 CA 和叶子证书缓存，线程安全（内部 `DashMap`）。
/// 通过 [`CaAuthority::load_or_create`] 初始化，之后可在多个线程间共享（`Arc`）。
pub struct CaAuthority {
    ca: StoredCa,
    /// 叶子证书缓存：域名 → `Arc<ServerConfig>`。
    leaf_cache: DashMap<String, Arc<ServerConfig>>,
}

impl CaAuthority {
    /// 从磁盘加载已有 CA，或首次启动时生成新的根 CA 并持久化。
    ///
    /// CA 文件存储位置：`~/Library/Application Support/Cuckoo/ca/`（macOS）
    /// - `ca.crt`：根 CA 证书（PEM 格式，方便用户安装）
    /// - `ca.key`：根 CA 私钥（PEM 格式，PKCS#8）
    pub async fn load_or_create() -> Result<Self, CaError> {
        let ca_dir = ca_dir_path();
        fs::create_dir_all(&ca_dir)?;

        let cert_path = ca_dir.join("ca.crt");
        let key_path = ca_dir.join("ca.key");

        if cert_path.exists() && key_path.exists() {
            // 加载已有 CA
            Self::load_from_files(&cert_path, &key_path)
        } else {
            // 首次启动：生成根 CA
            Self::generate_and_persist(&cert_path, &key_path)
        }
    }

    /// 从 PEM 文件加载已有 CA。
    ///
    /// 从磁盘加载时，用 PEM 重建 `rcgen::KeyPair`，然后用相同的参数重新
    /// 自签名 CA 证书。只要 CA 的 subject/公钥一致，签发的叶子证书就能被信任。
    fn load_from_files(cert_path: &PathBuf, key_path: &PathBuf) -> Result<Self, CaError> {
        let cert_pem = fs::read(cert_path)?;
        let key_pem = fs::read(key_path)?;

        // 解析 PEM 证书
        let cert_der = rustls_pemfile::certs(&mut &cert_pem[..])
            .next()
            .ok_or_else(|| CaError::LoadFailed("no certificate found in ca.crt".into()))?
            .map_err(|e| CaError::LoadFailed(format!("failed to parse CA cert PEM: {e}")))?
            .into_owned();

        // 重建 rcgen KeyPair
        let key_pair = KeyPair::from_pem(&String::from_utf8_lossy(&key_pem))
            .map_err(|e| CaError::LoadFailed(format!("failed to reconstruct KeyPair: {e}")))?;

        // 用相同的参数重新自签名 CA 证书
        let params = ca_cert_params();
        let ca_cert = params.self_signed(&key_pair)?;

        let ca = StoredCa {
            cert_der,
            key_pair,
            ca_cert,
        };

        tracing::info!("CA loaded from disk");

        Ok(Self {
            ca,
            leaf_cache: DashMap::new(),
        })
    }

    /// 首次启动：生成根 CA 并持久化到磁盘。
    fn generate_and_persist(cert_path: &PathBuf, key_path: &PathBuf) -> Result<Self, CaError> {
        let params = ca_cert_params();
        let key_pair = KeyPair::generate()?;
        let ca_cert = params.self_signed(&key_pair)?;

        // 序列化为 DER
        let cert_der = ca_cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        // 持久化为 PEM 格式
        let cert_pem = pem_encode(&cert_der, "CERTIFICATE");
        let key_pem = pem_encode(&key_der, "PRIVATE KEY");

        fs::write(cert_path, &cert_pem)?;
        fs::write(key_path, &key_pem)?;

        // 限制私钥文件权限（Unix）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(key_path, fs::Permissions::from_mode(0o600));
        }

        tracing::info!("Root CA generated and persisted to {:?}", cert_path);

        let ca = StoredCa {
            cert_der: CertificateDer::from(cert_der),
            key_pair,
            ca_cert,
        };

        Ok(Self {
            ca,
            leaf_cache: DashMap::new(),
        })
    }

    /// 获取或签发指定域名的叶子证书，返回缓存的 `ServerConfig`。
    ///
    /// 缓存命中时直接返回；未命中时用 `rcgen` 现场签发，构造 `ServerConfig` 并缓存。
    pub fn get_or_issue_server_config(&self, domain: &str) -> Result<Arc<ServerConfig>, CaError> {
        // 检查缓存
        if let Some(cached) = self.leaf_cache.get(domain) {
            return Ok(cached.clone());
        }

        // 签发叶子证书
        let server_config = self.sign_leaf_cert(domain)?;

        // 缓存
        self.leaf_cache
            .insert(domain.to_string(), server_config.clone());

        Ok(server_config)
    }

    /// 签发叶子证书并构造 `ServerConfig`。
    fn sign_leaf_cert(&self, domain: &str) -> Result<Arc<ServerConfig>, CaError> {
        // 叶子证书参数
        let mut params = CertificateParams::new(vec![domain.to_string()])?;
        params.distinguished_name = {
            let mut dn = DistinguishedName::new();
            dn.push(DnType::CommonName, domain);
            dn
        };
        // 叶子证书有效期 1 年
        params.not_before = rcgen::date_time_ymd(
            time::OffsetDateTime::now_utc().year(),
            time::OffsetDateTime::now_utc().month() as u8,
            time::OffsetDateTime::now_utc().day(),
        );
        params.not_after = rcgen::date_time_ymd(
            time::OffsetDateTime::now_utc().year() + 1,
            time::OffsetDateTime::now_utc().month() as u8,
            time::OffsetDateTime::now_utc().day(),
        );
        // 扩展：KeyUsage + ExtKeyUsage (serverAuth)
        params.key_usages = vec![
            rcgen::KeyUsagePurpose::DigitalSignature,
            rcgen::KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];

        // 生成叶子证书的 KeyPair
        let leaf_key_pair = KeyPair::generate()?;

        // 用根 CA 签发
        let leaf_cert = params.signed_by(&leaf_key_pair, &self.ca.ca_cert, &self.ca.key_pair)?;

        // 构造 rustls CertifiedKey
        let cert_chain = vec![CertificateDer::from(leaf_cert.der().to_vec())];
        let leaf_key_der = leaf_key_pair.serialize_der();
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            leaf_key_der.to_vec(),
        ));

        // 使用 rustls 的 ring crypto provider 构造签名密钥
        let provider = rustls::crypto::ring::default_provider();
        let certified_key = CertifiedKey::new(
            cert_chain,
            provider
                .key_provider
                .load_private_key(key)
                .map_err(|e| CaError::TlsConfigFailed(e.to_string()))?,
        );

        // 构造 ServerConfig
        let mut server_config = RustlsServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(SingleCertResolver::new(certified_key)));

        // 设置 ALPN 协议。
        // 注意：只声明 http/1.1 —— 代理内核目前只有 HTTP/1.1 解析器，
        // 若在这里声明 h2，rustls 会按顺序优先选中 h2，浏览器握手成功后
        // 发送 HTTP/2 二进制 preface，HTTP/1.1 状态机必然解析失败。
        // 待 http2.rs 落地后再加入 "h2"。
        server_config.alpn_protocols = vec![b"http/1.1".to_vec()];

        Ok(Arc::new(server_config))
    }

    /// 导出根 CA 证书（PEM 格式），供前端下载安装。
    pub fn export_ca_cert_pem(&self) -> Result<Vec<u8>, CaError> {
        Ok(pem_encode(&self.ca.cert_der, "CERTIFICATE").into_bytes())
    }

    /// 导出根 CA 证书（DER 格式）。
    pub fn export_ca_cert_der(&self) -> Result<Vec<u8>, CaError> {
        Ok(self.ca.cert_der.to_vec())
    }

    /// 获取根 CA 证书的 rustls `RootCertStore`，供 `reqwest` / `cuckoo-http` 信任本代理签发的证书。
    pub fn root_cert_store(&self) -> RootCertStore {
        let mut store = RootCertStore::empty();
        store.add(self.ca.cert_der.clone()).ok();
        store
    }

    /// 构建 `TlsAcceptor`，用于代理的 TLS 终止。
    ///
    /// 注意：这个 acceptor 使用一个默认的 cert resolver，
    /// 实际使用时应该通过 `get_or_issue_server_config` 动态获取。
    /// 这个方法主要供测试使用。
    pub fn tls_acceptor(&self, domain: &str) -> Result<TlsAcceptor, CaError> {
        let server_config = self.get_or_issue_server_config(domain)?;
        Ok(TlsAcceptor::from(server_config))
    }

    /// 清除叶子证书缓存（例如 CA 被重新生成时调用）。
    pub fn clear_cache(&self) {
        self.leaf_cache.clear();
    }
}

/// 构造根 CA 证书参数。
fn ca_cert_params() -> CertificateParams {
    let mut params = CertificateParams::new(vec![]).unwrap_or_default();
    params.distinguished_name = {
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Cuckoo Proxy CA");
        dn.push(DnType::OrganizationName, "Cuckoo");
        dn
    };
    // CA 证书有效期为 10 年
    let now = time::OffsetDateTime::now_utc();
    params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day());
    params.not_after = rcgen::date_time_ymd(now.year() + 10, now.month() as u8, now.day());
    // 标记为 CA 证书
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    params
}

/// 单证书 resolver：为 rustls 提供固定的 `CertifiedKey`。
#[derive(Debug)]
struct SingleCertResolver {
    key: Arc<CertifiedKey>,
}

impl SingleCertResolver {
    fn new(key: CertifiedKey) -> Self {
        Self {
            key: Arc::new(key),
        }
    }
}

impl rustls::server::ResolvesServerCert for SingleCertResolver {
    fn resolve(&self, _client_hello: rustls::server::ClientHello) -> Option<Arc<CertifiedKey>> {
        Some(self.key.clone())
    }
}

/// 将 DER 数据编码为 PEM 格式字符串。
fn pem_encode(der: &[u8], label: &str) -> String {
    use std::fmt::Write;
    const LINE_WIDTH: usize = 64;

    let b64 = base64_encode(der);
    let mut pem = String::new();
    let _ = writeln!(pem, "-----BEGIN {label}-----");
    for chunk in b64.as_bytes().chunks(LINE_WIDTH) {
        pem.push_str(std::str::from_utf8(chunk).unwrap());
        pem.push('\n');
    }
    let _ = writeln!(pem, "-----END {label}-----");
    pem
}

/// 简单的 Base64 编码（不引入额外依赖）。
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((n >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// CA 文件存储目录：`~/Library/Application Support/Cuckoo/ca/`（macOS）
fn ca_dir_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("Cuckoo").join("ca")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_and_sign() {
        // 测试 base64 和 pem 编码
        let der = b"hello world";
        let b64 = base64_encode(der);
        assert_eq!(b64, "aGVsbG8gd29ybGQ=");

        let pem = pem_encode(der, "TEST");
        assert!(pem.starts_with("-----BEGIN TEST-----"));
        assert!(pem.ends_with("-----END TEST-----\n"));
    }

    #[tokio::test]
    async fn test_ca_generate_and_leaf_sign() {
        // 测试 CA 生成和叶子证书签发
        let params = ca_cert_params();
        let key_pair = KeyPair::generate().unwrap();
        let ca_cert = params.self_signed(&key_pair).unwrap();

        // 签发叶子证书
        let mut leaf_params =
            CertificateParams::new(vec!["example.com".to_string()]).unwrap();
        leaf_params.distinguished_name = {
            let mut dn = DistinguishedName::new();
            dn.push(DnType::CommonName, "example.com");
            dn
        };
        let now = time::OffsetDateTime::now_utc();
        leaf_params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day());
        leaf_params.not_after =
            rcgen::date_time_ymd(now.year() + 1, now.month() as u8, now.day());

        let leaf_key = KeyPair::generate().unwrap();
        let leaf_cert = leaf_params
            .signed_by(&leaf_key, &ca_cert, &key_pair)
            .unwrap();

        // 验证证书不为空
        assert!(!leaf_cert.der().is_empty());
    }
}
