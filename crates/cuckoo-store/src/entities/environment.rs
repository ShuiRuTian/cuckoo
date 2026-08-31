//! `Environment` Entity（`spec.md` 3.2 节）。
//!
//! 使用 SeaORM 2.0 `#[sea_orm::model]` 宏。
//!
//! 注意：Entity 上不再导出 `ts-rs` TS 类型——前端类型由 `cuckoo-dto` 定义。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 复合类型：环境变量条目（以 JSON 列存储）。
///
/// 注意：此类型为数据库内部表示，不导出 TS 类型。
/// 前端可见的同名类型定义在 `cuckoo-dto` 中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVariable {
    pub key: String,
    pub value: String,
    pub secret: bool,
    pub enabled: bool,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "environment")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    /// JSON 列：`Vec<EnvVariable>`
    pub variables: Json,
    /// 所属 Workspace（多对一）。
    #[sea_orm(belongs_to, from = "workspace_id", to = "id")]
    pub workspace: BelongsTo<super::workspace::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
