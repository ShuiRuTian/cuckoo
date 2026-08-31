//! `cuckoo-core`：Tauri 无关的公共类型、错误处理与 RPC 方法登记表。
//!
//! 这是整个 workspace 里唯一允许被几乎所有其他 crate 依赖的基础 crate
//! （`spec.md` 2.1 节），不包含任何具体业务逻辑，只提供：
//! - [`error`]：所有 Service 方法与 HTTP handler 共用的错误类型
//! - [`rpc_registry`]：`#[rpc_method]` 宏（见 `cuckoo-macros`）使用的编译期方法登记表

pub mod error;
pub mod rpc_registry;

pub use error::{ServiceError, ServiceResult};
