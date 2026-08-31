//! `cuckoo-platform`：系统集成（代理设置、CA 信任安装，分平台实现）。
//!
//! 基于 `plan.md` M2.4 节 / `spec.md` 4.7 节：
//! - macOS：通过 `networksetup` 命令设置/清除系统 HTTP/HTTPS 代理
//! - Windows/Linux：TODO stub，M5 阶段补齐
//!
//! ## macOS `networksetup` 用法
//!
//! ```sh
//! # 获取第一个网络服务名称
//! networksetup -listallnetworkservices
//!
//! # 设置 Web 代理（HTTP）
//! networksetup -setwebproxy <networkservice> <domain> <portnumber>
//! networksetup -setwebproxy "Wi-Fi" 127.0.0.1 8080
//!
//! # 设置 Secure Web 代理（HTTPS）
//! networksetup -setsecurewebproxy <networkservice> <domain> <portnumber>
//! networksetup -setsecurewebproxy "Wi-Fi" 127.0.0.1 8080
//!
//! # 关闭 Web 代理
//! networksetup -setwebproxystate <networkservice> off
//!
//! # 关闭 Secure Web 代理
//! networksetup -setsecurewebproxystate <networkservice> off
//! ```

use cuckoo_core::ServiceResult;

pub mod macos;
pub mod stub;

pub use macos::MacOsProxyManager;
pub use stub::StubProxyManager;

/// 系统代理管理器 trait：统一各平台的代理设置接口。
pub trait SystemProxyManager: Send + Sync {
    /// 设置系统 HTTP/HTTPS 代理指向 `host:port`。
    fn set_proxy(&self, host: &str, port: u16) -> ServiceResult<()>;

    /// 清除（恢复）系统代理设置。
    fn clear_proxy(&self) -> ServiceResult<()>;

    /// 查询当前代理是否已启用。
    fn is_proxy_enabled(&self) -> ServiceResult<bool>;

    /// 平台名称（如 "macOS"、"Linux"）。
    fn platform_name(&self) -> &'static str;
}

/// 选择当前平台合适的代理管理器。
pub fn create_proxy_manager() -> Box<dyn SystemProxyManager> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacOsProxyManager::new())
    }
    #[cfg(target_os = "windows")]
    {
        // M5: Windows 分支补齐
        Box::new(StubProxyManager::new("Windows"))
    }
    #[cfg(target_os = "linux")]
    {
        // M5: Linux 分支补齐
        Box::new(StubProxyManager::new("Linux"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Box::new(StubProxyManager::new("Unknown"))
    }
}

/// 保存设置代理前的原始状态，用于恢复。
#[derive(Debug, Clone, Default)]
pub struct ProxySnapshot {
    /// 设置代理前的网络服务名称列表及其代理状态
    pub services: Vec<ServiceProxyState>,
}

/// 单个网络服务的代理状态快照。
#[derive(Debug, Clone, Default)]
pub struct ServiceProxyState {
    pub service_name: String,
    pub web_proxy_enabled: bool,
    pub web_proxy_host: Option<String>,
    pub web_proxy_port: Option<u16>,
    pub secure_proxy_enabled: bool,
    pub secure_proxy_host: Option<String>,
    pub secure_proxy_port: Option<u16>,
}
