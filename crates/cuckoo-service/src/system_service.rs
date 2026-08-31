//! `system_service`：系统级操作——CA 证书导出、系统代理设置
//! （`spec.md` 7.2 节，`plan.md` M2.1 节）。
//!
//! 当前实现：
//! - `export_ca_cert()`：返回根 CA 证书 PEM 内容 + 指纹信息，供前端下载安装。
//!
//! 后续补充（`plan.md` M2.4 节）：
//! - `set_system_proxy()` / `unset_system_proxy()`：通过 `cuckoo-platform` 操作系统代理设置。

use cuckoo_ca::CaAuthority;
use cuckoo_core::{ServiceError, ServiceResult};
use cuckoo_dto::CaCertInfo;
use cuckoo_macros::rpc_method;
use sha2::{Digest, Sha256};

/// 导出根 CA 证书信息，供前端下载并引导用户安装到系统信任链。
///
/// 标注为 `POST /api/certs/export`（`spec.md` 7.2 节，`plan.md` M2.1 节）。
///
/// 返回 `CaCertInfo`，包含 PEM 文本、SHA-256 指纹和 Common Name。
#[rpc_method("POST", "/api/certs/export")]
pub async fn export_ca_cert(ca: &CaAuthority) -> ServiceResult<CaCertInfo> {
    let pem = ca.export_ca_cert_pem()?;
    let der = ca.export_ca_cert_der()?;

    // 计算 SHA-256 指纹
    let fingerprint = sha256_hex(&der);

    Ok(CaCertInfo {
        pem: String::from_utf8(pem)
            .map_err(|e| ServiceError::Internal(format!("CA PEM is not valid UTF-8: {e}")))?,
        fingerprint,
        common_name: "Cuckoo Proxy CA".to_string(),
    })
}

/// 计算 SHA-256 哈希并返回十六进制字符串。
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;

    let mut hasher = Sha256::new();
    hasher.update(data);
    let bytes = hasher.finalize();

    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes.iter() {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
