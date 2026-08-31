//! `Folder` Entity（`spec.md` 3.2 节）。
//!
//! 使用 SeaORM 2.0 `#[sea_orm::model]` 宏，关系直接以字段形式定义在 Model 上。
//! Folder 支持自引用（parent_folder_id -> 自身 id）。
//!
//! 注意：Entity 上不再导出 `ts-rs` TS 类型——前端类型由 `cuckoo-dto` 定义。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "folder")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub workspace_id: String,
    /// `None` 表示顶层 Folder；有值表示嵌套在某个 Folder 下。
    pub parent_folder_id: Option<String>,
    pub name: String,
    /// 支持拖拽排序的浮点排序键。
    pub sort_key: f64,
    /// 所属 Workspace（多对一）。
    #[sea_orm(belongs_to, from = "workspace_id", to = "id")]
    pub workspace: BelongsTo<super::workspace::Entity>,
    /// 父 Folder（自引用，多对一）。
    #[sea_orm(
        self_ref,
        relation_enum = "ParentFolder",
        relation_reverse = "ChildFolders",
        from = "parent_folder_id",
        to = "id"
    )]
    pub parent_folder: BelongsTo<Option<Entity>>,
    /// 子 Folders（自引用，一对多）。
    #[sea_orm(self_ref, relation_enum = "ChildFolders", relation_reverse = "ParentFolder")]
    pub child_folders: HasMany<Entity>,
    /// Folder 下的请求定义（一对多）。
    #[sea_orm(has_many)]
    pub http_request_defs: HasMany<super::http_request_def::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
