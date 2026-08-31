//! macOS 系统代理管理：通过 `networksetup` 命令设置/清除 HTTP/HTTPS 代理。
//!
//! `networksetup` 是 macOS 自带的网络配置命令行工具，无需额外安装。
//! 它需要 root 权限来修改系统代理设置（实际操作中 Tauri 应用会通过
//! `osascript` 弹出授权对话框，或者通过 `sudo` 提示用户输入密码）。
//!
//! M2 阶段简化方案：直接调用 `networksetup`，如果权限不足则返回错误
//! 并提示用户手动设置。M5 阶段可以接入 `osascript` 实现一键授权。

use std::sync::Mutex;

use cuckoo_core::{ServiceError, ServiceResult};

use crate::{ServiceProxyState, SystemProxyManager, ProxySnapshot};

/// macOS 代理管理器。
///
/// 内部保存设置代理前的快照，`clear_proxy()` 时恢复。
pub struct MacOsProxyManager {
    /// 保存代理前的快照（线程安全）
    snapshot: Mutex<Option<ProxySnapshot>>,
}

impl MacOsProxyManager {
    pub fn new() -> Self {
        Self {
            snapshot: Mutex::new(None),
        }
    }

    /// 获取所有网络服务名称。
    ///
    /// `networksetup -listallnetworkservices` 输出格式：
    /// ```text
    /// An asterisk (*) denotes that a network service is disabled.
    /// Wi-Fi
    /// Ethernet Adapter
    /// Thunderbolt Bridge
    /// ```
    fn list_network_services() -> ServiceResult<Vec<String>> {
        let output = std::process::Command::new("networksetup")
            .arg("-listallnetworkservices")
            .output()
            .map_err(|e| ServiceError::Internal(format!("failed to run networksetup: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ServiceError::Internal(format!(
                "networksetup -listallnetworkservices failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // 第一行是说明文字，跳过；带 * 的已禁用，也跳过
        let services: Vec<String> = stdout
            .lines()
            .skip(1) // 跳过 "An asterisk (*) denotes..."
            .filter(|line| !line.is_empty() && !line.starts_with('*'))
            .map(|line| line.trim_start_matches("* ").to_string())
            .collect();

        Ok(services)
    }

    /// 获取指定网络服务的 Web Proxy (HTTP) 状态。
    ///
    /// `networksetup -getwebproxy "Wi-Fi"` 输出：
    /// ```text
    /// Enabled: No
    /// Server: 
    /// Port: 0
    /// Authenticated Proxy Enabled: 0
    /// ...
    /// ```
    fn get_web_proxy(service: &str) -> ServiceResult<ServiceProxyState> {
        let mut state = ServiceProxyState {
            service_name: service.to_string(),
            ..Default::default()
        };

        // HTTP proxy
        let output = Self::run_networksetup(&["-getwebproxy", service])?;
        Self::parse_proxy_output(&output, &mut state, false);

        // HTTPS proxy
        let output = Self::run_networksetup(&["-getsecurewebproxy", service])?;
        Self::parse_proxy_output(&output, &mut state, true);

        Ok(state)
    }

    /// 执行 `networksetup` 命令并返回 stdout。
    fn run_networksetup(args: &[&str]) -> ServiceResult<String> {
        let output = std::process::Command::new("networksetup")
            .args(args)
            .output()
            .map_err(|e| ServiceError::Internal(format!("failed to run networksetup: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ServiceError::Internal(format!(
                "networksetup {} failed: {stderr}",
                args.join(" ")
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// 解析 `networksetup -getwebproxy` 输出。
    fn parse_proxy_output(output: &str, state: &mut ServiceProxyState, is_secure: bool) {
        for line in output.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("Enabled:") {
                let enabled = val.trim().eq_ignore_ascii_case("Yes");
                if is_secure {
                    state.secure_proxy_enabled = enabled;
                } else {
                    state.web_proxy_enabled = enabled;
                }
            } else if let Some(val) = line.strip_prefix("Server:") {
                let host = val.trim();
                if !host.is_empty() {
                    if is_secure {
                        state.secure_proxy_host = Some(host.to_string());
                    } else {
                        state.web_proxy_host = Some(host.to_string());
                    }
                }
            } else if let Some(val) = line.strip_prefix("Port:") {
                if let Ok(port) = val.trim().parse::<u16>() {
                    if port > 0 {
                        if is_secure {
                            state.secure_proxy_port = Some(port);
                        } else {
                            state.web_proxy_port = Some(port);
                        }
                    }
                }
            }
        }
    }

    /// 保存当前所有网络服务的代理状态快照。
    fn save_snapshot(&self) -> ServiceResult<ProxySnapshot> {
        let services = Self::list_network_services()?;
        let mut states = Vec::new();

        for service in &services {
            match Self::get_web_proxy(service) {
                Ok(state) => states.push(state),
                Err(e) => {
                    tracing::warn!(service = %service, ?e, "failed to get proxy state for service");
                }
            }
        }

        Ok(ProxySnapshot { services: states })
    }
}

impl Default for MacOsProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProxyManager for MacOsProxyManager {
    fn set_proxy(&self, host: &str, port: u16) -> ServiceResult<()> {
        // 先保存快照（如果还没有保存过）
        {
            let mut guard = self.snapshot.lock().unwrap();
            if guard.is_none() {
                let snap = self.save_snapshot()?;
                tracing::info!("saved proxy snapshot before setting proxy");
                *guard = Some(snap);
            }
        }

        let services = Self::list_network_services()?;
        let port_str = port.to_string();

        for service in &services {
            // 设置 HTTP 代理
            if let Err(e) = Self::run_networksetup(&[
                "-setwebproxy",
                service,
                host,
                &port_str,
            ]) {
                tracing::warn!(service = %service, ?e, "failed to set web proxy");
            }

            // 设置 HTTPS 代理
            if let Err(e) = Self::run_network_setup(&[
                "-setsecurewebproxy",
                service,
                host,
                &port_str,
            ]) {
                tracing::warn!(service = %service, ?e, "failed to set secure web proxy");
            }

            tracing::info!(service = %service, host = %host, port, "proxy set");
        }

        Ok(())
    }

    fn clear_proxy(&self) -> ServiceResult<()> {
        // 恢复快照（如果有）
        let snapshot = {
            let mut guard = self.snapshot.lock().unwrap();
            guard.take()
        };

        if let Some(snapshot) = snapshot {
            // 有快照：恢复原始状态
            for service_state in &snapshot.services {
                let name = &service_state.service_name;

                if service_state.web_proxy_enabled {
                    // 原来是启用的，恢复原来的代理设置
                    if let (Some(ref host), Some(port)) = (
                        &service_state.web_proxy_host,
                        service_state.web_proxy_port,
                    ) {
                        let port_str = port.to_string();
                        let _ = Self::run_networksetup(&[
                            "-setwebproxy",
                            name,
                            host,
                            &port_str,
                        ]);
                    }
                } else {
                    // 原来是禁用的，关闭代理
                    let _ = Self::run_networksetup(&["-setwebproxystate", name, "off"]);
                }

                if service_state.secure_proxy_enabled {
                    if let (Some(ref host), Some(port)) = (
                        &service_state.secure_proxy_host,
                        service_state.secure_proxy_port,
                    ) {
                        let port_str = port.to_string();
                        let _ = Self::run_networksetup(&[
                            "-setsecurewebproxy",
                            name,
                            host,
                            &port_str,
                        ]);
                    }
                } else {
                    let _ = Self::run_networksetup(&["-setsecurewebproxystate", name, "off"]);
                }

                tracing::info!(service = %name, "proxy restored from snapshot");
            }
        } else {
            // 没有快照：直接关闭所有代理
            let services = Self::list_network_services()?;
            for service in &services {
                let _ = Self::run_networksetup(&["-setwebproxystate", service, "off"]);
                let _ = Self::run_networksetup(&["-setsecurewebproxystate", service, "off"]);
                tracing::info!(service = %service, "proxy disabled (no snapshot)");
            }
        }

        Ok(())
    }

    fn is_proxy_enabled(&self) -> ServiceResult<bool> {
        let services = Self::list_network_services()?;
        for service in &services {
            let state = Self::get_web_proxy(service)?;
            if state.web_proxy_enabled || state.secure_proxy_enabled {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn platform_name(&self) -> &'static str {
        "macOS"
    }
}

// 避免 run_network_setup 拼写错误
impl MacOsProxyManager {
    #[allow(non_snake_case)]
    fn run_network_setup(args: &[&str]) -> ServiceResult<String> {
        Self::run_networksetup(args)
    }
}
