//! `cuckoo-dto`：前端可见的 DTO（Data Transfer Object）类型层。
//!
//! 本 crate 是 **API 契约的唯一来源**——所有通过 `ts-rs` 导出给前端的
//! TypeScript 类型都定义在此处，而非 Entity 上。
//!
//! ## 层次关系
//!
//! Frontend (TypeScript) <-- ts-rs export -- cuckoo-dto -- depends on --> cuckoo-store (Entity / SeaORM)
//!
//! cuckoo-dto 定义:
//! - WorkspaceDto, FolderDto, HttpRequestDefDto, EnvironmentDto
//! - CreateXxxInput, UpdateXxxInput
//! - SendRequestInput, AdHocRequest, ExecuteRequestInput, ExecutionResult, PongResponse
//!
//! ## 设计原则
//!
//! - Entity 不导出 TS 类型：cuckoo-store 的 Model 上不再有 #[ts(export)]，
//!   数据库结构变更不会直接影响 API 契约。
//! - DTO 字段使用强类型：Entity 中的 Json 列在 DTO 中映射为具体的
//!   Vec<HeaderEntry> 等类型，而非 serde_json::Value。
//! - 转换逻辑集中在 From<Entity> 实现：Service 层只需 .into() 即可完成转换。

pub mod types;
pub mod convert;

// 重导出常用类型，方便外部使用
pub use types::*;
