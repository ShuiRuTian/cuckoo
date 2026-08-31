//! `cuckoo-service`：★ Service 层，唯一包含业务逻辑的地方（`spec.md` 2.1/2.2 节）。
//!
//! 函数签名不出现任何 Tauri 或其他传输层专属类型；`cuckoo-server` 直接调用这里的
//! 方法并通过 `#[rpc_method]` 宏自动生成对应 REST 路由。
//!
//! 所有 Service 方法的返回值和入参均使用 `cuckoo-dto` 或 `cuckoo-flow` 中定义的 DTO 类型，
//! 不直接暴露 Entity Model。
//!
//! M0 阶段只有 [`ping_service`] 一个占位模块用于打通端到端闭环；从 M1 起按
//! `plan.md` 逐步补齐：
//! - `request_service`：`send_request()`/`replay_flow()` 等
//! - `collection_service`：Workspace/Folder/Request/Environment CRUD
//! - `proxy_service`：`start_proxy()`/`stop_proxy()`/`subscribe_flows()`
//! - `rule_service`：拦截规则 CRUD、`resume_intercept()`
//! - `system_service`：证书导出、系统代理设置

pub mod collection_service;
pub mod ping_service;
pub mod proxy_service;
pub mod request_service;
pub mod rule_service;
pub mod system_service;

pub use ping_service::ping;
pub use request_service::send_request;
