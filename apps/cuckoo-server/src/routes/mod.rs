//! REST 路由挂载。
//!
//! M0 阶段方法数量很少，按 `plan.md` 的指导"先手写一份简单的清单式方案，
//! 不必一开始就追求完全自动反射"——每个 Service 模块提供一个
//! `xxx_router()` 函数，这里手动 `.merge()` 起来；`#[rpc_method]` 宏登记的
//! 元信息（见 `cuckoo_core::rpc_registry`）用于启动期打印路由表做人工核对。

pub mod collection;
pub mod flow;
pub mod ping;
pub mod proxy;
pub mod request_service;
pub mod rule;
pub mod system;

use axum::Router;

use crate::auth::AuthState;

/// 拼装所有业务 API 路由。新增一个 Service 模块时，在这里加一行 `.merge()`。
pub fn api_router() -> Router<AuthState> {
    Router::new()
        .merge(ping::router())
        .merge(collection::router())
        .merge(request_service::router())
        .merge(system::router())
        .merge(flow::router())
        .merge(proxy::router())
        .merge(rule::router())
}
