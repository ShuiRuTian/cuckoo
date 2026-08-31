//! 持有 `cuckoo-server` 的端口/token 信息，供 [`crate::system_commands::get_server_token`]
//! 等 Tauri command 读取（`spec.md` 2.1 节 `state.rs` 职责说明）。

pub struct ServerState {
    pub addr: std::net::SocketAddr,
    pub token: String,
    /// server 句柄：应用退出时调用其 `shutdown()` 做清理
    /// （停代理、恢复系统代理、取消挂起断点）。
    pub shutdown_handle: std::sync::Mutex<Option<cuckoo_server::ServerHandle>>,
}
