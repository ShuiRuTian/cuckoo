//! `Workspace` Entity（`spec.md` 3.2 节）。
//!
//! 使用 SeaORM 2.0 `#[sea_orm::model]` 宏，关系直接以字段形式定义在 Model 上，
//! 无需手写 `Relation` enum 和 `impl Related`。
//!
//! 关系字段使用 `HasMany<Entity>` 等包装类型。
//!
//! 注意：Entity 上不再导出 `ts-rs` TS 类型——前端类型由 `cuckoo-dto` 定义。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 复合类型：Header 条目（以 JSON 列存储）。
///
/// 注意：此类型为数据库内部表示，不导出 TS 类型。
/// 前端可见的同名类型定义在 `cuckoo-dto` 中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
    pub enabled: bool,
}

/// 复合类型：Workspace 设置（以 JSON 列存储）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceSettings {
    pub verify_tls: bool,
    pub timeout_ms: Option<u64>,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub name: String,
    /// JSON 列：`Vec<HeaderEntry>`
    pub base_headers: Json,
    /// JSON 列：`WorkspaceSettings`
    pub settings: Json,
    pub created_at: i64,
    pub updated_at: i64,
    #[sea_orm(has_many)]
    pub folders: HasMany<super::folder::Entity>,
    #[sea_orm(has_many)]
    pub http_request_defs: HasMany<super::http_request_def::Entity>,
    #[sea_orm(has_many)]
    pub environments: HasMany<super::environment::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
