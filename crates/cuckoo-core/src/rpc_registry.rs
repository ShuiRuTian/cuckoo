//! `#[rpc_method]` 宏（见 `cuckoo-macros`）在编译期把每个标注方法登记进这里的全局清单。
//!
//! 设计思路（M0 最小实现，`spec.md` 2.3 节的第一步）：
//! - `cuckoo-macros` 不需要知道 `axum::Router` 具体怎么拼装，它只负责把一个
//!   "如何把这个方法追加到 Router 上"的注册函数指针，连同 method/path 元信息一起
//!   通过 `inventory::submit!` 收集起来。
//! - `cuckoo-server` 启动时调用 [`build_router`]，遍历所有登记项，逐个调用其
//!   `register` 闭包，得到最终的 `axum::Router`。
//! - 这样新增一个 Service 方法只需要写函数 + 加一个属性宏，不需要在 `cuckoo-server`
//!   里手写路由注册代码（消灭 2.3 节说的"胶水代码"）。
//!
//! 后续演进（M1+）：`RpcMethodDescriptor` 可以再加入入参/返回类型的 `schemars`/`ts-rs`
//! 元信息，供 `build.rs` 生成前端 TS 客户端、CLI 通用调用入口、MCP tool schema。

use axum::Router;

/// 一条 Service 方法在编译期登记的元信息。
pub struct RpcMethodDescriptor {
    /// HTTP method，如 "GET" / "POST"。
    pub method: &'static str,
    /// 路由路径，如 "/api/ping"。
    pub path: &'static str,
    /// 原始 Rust 函数名，主要用于调试/日志。
    pub fn_name: &'static str,
}

inventory::collect!(RpcMethodDescriptor);

/// 重新导出 `inventory`，方便 `cuckoo-macros` 生成的代码里使用
/// `::cuckoo_core::rpc_registry::inventory::submit!`，不需要 `cuckoo-service`
/// 单独添加 `inventory` 依赖。
pub use inventory;

/// 遍历所有通过 `#[rpc_method]` 登记的方法描述，主要供 `cuckoo-server` 打印路由表 /
/// 做启动期自检使用。真正的路由挂载由各 Service 模块自己提供的
/// `axum::Router` 构建函数完成（见 `cuckoo-service::ping_router()` 等）。
pub fn all_descriptors() -> Vec<&'static RpcMethodDescriptor> {
    inventory::iter::<RpcMethodDescriptor>().collect()
}

/// 空占位，保留给未来"完全自动拼装 Router"的实现使用。
/// M0 阶段 `cuckoo-server` 直接手动 `.merge()` 各 Service 模块导出的
/// 子 Router，同时用 [`all_descriptors`] 打印出的清单做人工核对，
/// 避免一开始就实现复杂的反射式自动注册。
pub fn build_router() -> Router {
    Router::new()
}
