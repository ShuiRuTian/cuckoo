//! Windows/Linux 平台的占位实现（M5 阶段补齐）。

use cuckoo_core::{ServiceError, ServiceResult};

use crate::SystemProxyManager;

/// 占位代理管理器：所有操作返回 "not supported"。
pub struct StubProxyManager {
    platform: &'static str,
}

impl StubProxyManager {
    pub fn new(platform: &'static str) -> Self {
        Self { platform }
    }
}

impl SystemProxyManager for StubProxyManager {
    fn set_proxy(&self, _host: &str, _port: u16) -> ServiceResult<()> {
        Err(ServiceError::Internal(format!(
            "system proxy not yet implemented for {}",
            self.platform
        )))
    }

    fn clear_proxy(&self) -> ServiceResult<()> {
        Err(ServiceError::Internal(format!(
            "system proxy not yet implemented for {}",
            self.platform
        )))
    }

    fn is_proxy_enabled(&self) -> ServiceResult<bool> {
        Ok(false)
    }

    fn platform_name(&self) -> &'static str {
        self.platform
    }
}
