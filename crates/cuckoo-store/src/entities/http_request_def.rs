//! `HttpRequestDef` Entity（`spec.md` 3.2 节）。
//!
//! Collection 里保存的"请求模板"。使用 SeaORM 2.0 `#[sea_orm::model]` 宏。
//!
//! 注意：Entity 上不再导出 `ts-rs` TS 类型——前端类型由 `cuckoo-dto` 定义。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 复合类型：Query Param 条目（以 JSON 列存储）。
///
/// 注意：此类型为数据库内部表示，不导出 TS 类型。
/// 前端可见的同名类型定义在 `cuckoo-dto` 中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// 请求体类型（M1 先做 Raw JSON，其他后续补）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Raw {
        content_type: String,
        text: String,
    },
}

/// 认证配置（M1 先做 None/Basic/Bearer/ApiKey，其他后续补）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    #[default]
    None,
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key_name: String,
        key_value: String,
        add_to: String, // "header" | "query"
    },
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "http_request_def")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub folder_id: Option<String>,
    pub workspace_id: String,
    pub name: String,
    pub method: String,
    /// 含 `{{variable}}` 模板语法。
    pub url: String,
    /// JSON 列：`Vec<HeaderEntry>`
    pub headers: Json,
    /// JSON 列：`Vec<KeyValueEntry>`
    pub query_params: Json,
    /// JSON 列：`RequestBody`
    pub body: Json,
    /// JSON 列：`AuthConfig`
    pub auth: Json,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub sort_key: f64,
    /// 所属 Workspace（多对一）。
    #[sea_orm(belongs_to, from = "workspace_id", to = "id")]
    pub workspace: BelongsTo<super::workspace::Entity>,
    /// 所属 Folder（多对一，可选）。
    #[sea_orm(belongs_to, from = "folder_id", to = "id")]
    pub folder: BelongsTo<Option<super::folder::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
