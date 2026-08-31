//! 所有前端可见的 DTO 类型定义。
//!
//! 这些类型通过 `ts-rs` 的 `#[ts(export)]` 生成对应的 TypeScript 文件，
//! 供前端 `import` 使用。
//!
//! ## 命名约定
//!
//! - 实体 DTO：`WorkspaceDto`、`FolderDto`、`HttpRequestDefDto`、`EnvironmentDto`
//!   （前端 TS 中通过 `#[ts(rename)]` 映射为 `Workspace`、`Folder` 等）
//! - 输入 DTO：`CreateWorkspaceInput`、`UpdateWorkspaceInput` 等
//! - 功能 DTO：`SendRequestInput`、`AdHocRequest`、`ExecuteRequestInput`、`ExecutionResult`、`PongResponse`

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── 共享复合类型 ──────────────────────────────────────────────

/// Header 条目（前端可见的复合类型）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
    pub enabled: bool,
}

/// Workspace 设置（前端可见的复合类型）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct WorkspaceSettings {
    pub verify_tls: bool,
    #[ts(type = "number | null")]
    pub timeout_ms: Option<u64>,
}

/// Query Param 条目（前端可见的复合类型）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct KeyValueEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

/// 请求体类型（M1 先做 Raw JSON，其他后续补）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Raw {
        content_type: String,
        text: String,
    },
}

/// 认证配置（M1 先做 None/Basic/Bearer/ApiKey）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
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

/// 环境变量条目（前端可见的复合类型）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct EnvVariable {
    pub key: String,
    pub value: String,
    pub secret: bool,
    pub enabled: bool,
}

// ─── 实体 DTO ──────────────────────────────────────────────────

/// Workspace 的前端 DTO。
///
/// Entity 中的 `Json` 列在 DTO 中映射为强类型字段，
/// 关系字段（`folders` 等）不暴露给前端。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/", rename = "WorkspaceModel")]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub base_headers: Vec<HeaderEntry>,
    pub settings: WorkspaceSettings,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// Folder 的前端 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/", rename = "FolderModel")]
pub struct FolderDto {
    pub id: String,
    pub workspace_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
    pub sort_key: f64,
}

/// HttpRequestDef 的前端 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/", rename = "HttpRequestDefModel")]
pub struct HttpRequestDefDto {
    pub id: String,
    pub folder_id: Option<String>,
    pub workspace_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub query_params: Vec<KeyValueEntry>,
    pub body: RequestBody,
    pub auth: AuthConfig,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
    pub sort_key: f64,
}

/// Environment 的前端 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/", rename = "EnvironmentModel")]
pub struct EnvironmentDto {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub variables: Vec<EnvVariable>,
}

// ─── 输入 DTO ──────────────────────────────────────────────────

/// 创建 Workspace 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CreateWorkspaceInput {
    pub name: String,
    pub base_headers: Vec<HeaderEntry>,
    pub settings: WorkspaceSettings,
}

/// 更新 Workspace 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct UpdateWorkspaceInput {
    pub name: Option<String>,
    pub base_headers: Option<Vec<HeaderEntry>>,
    pub settings: Option<WorkspaceSettings>,
}

/// 创建 Folder 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CreateFolderInput {
    pub workspace_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
}

/// 更新 Folder 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct UpdateFolderInput {
    pub name: Option<String>,
    pub parent_folder_id: Option<Option<String>>,
    pub sort_key: Option<f64>,
}

/// 创建 HTTP 请求定义的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CreateRequestInput {
    pub workspace_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub query_params: Vec<KeyValueEntry>,
    pub body: RequestBody,
    pub auth: AuthConfig,
}

/// 更新 HTTP 请求定义的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct UpdateRequestInput {
    pub folder_id: Option<Option<String>>,
    pub name: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<HeaderEntry>>,
    pub query_params: Option<Vec<KeyValueEntry>>,
    pub body: Option<RequestBody>,
    pub auth: Option<AuthConfig>,
    pub sort_key: Option<f64>,
}

/// 创建 Environment 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CreateEnvironmentInput {
    pub workspace_id: String,
    pub name: String,
    pub variables: Vec<EnvVariable>,
}

/// 更新 Environment 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct UpdateEnvironmentInput {
    pub name: Option<String>,
    pub variables: Option<Vec<EnvVariable>>,
}

// ─── 功能 DTO ──────────────────────────────────────────────────

/// `send_request` 的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct SendRequestInput {
    pub request_id: Option<String>,
    pub ad_hoc: Option<AdHocRequest>,
    pub environment_id: Option<String>,
    /// M3.3：是否经过本地 MITM 代理转发（调试请求在代理规则下的行为）。
    /// 仅在代理运行中时生效。
    pub via_proxy: Option<bool>,
}

/// Ad-hoc 请求参数（不经数据库，直接发送）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct AdHocRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub query_params: Vec<KeyValueEntry>,
    pub body: RequestBody,
    pub auth: AuthConfig,
}

/// 执行 HTTP 请求的输入参数（内部传递，变量已插值完毕）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct ExecuteRequestInput {
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub query_params: Vec<KeyValueEntry>,
    pub body: RequestBody,
    pub auth: AuthConfig,
}

/// HTTP 请求执行结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct ExecutionResult {
    pub status: u16,
    pub status_text: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: String,
    pub body_size: usize,
    pub content_type: Option<String>,
    #[ts(type = "number")]
    pub total_time_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// `ping()` 的返回值。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct PongResponse {
    pub message: String,
    #[ts(type = "number")]
    pub server_time_ms: i64,
}

/// CA 证书信息（导出端点返回）。
///
/// 前端用此信息引导用户安装根 CA 证书。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CaCertInfo {
    /// PEM 格式的根 CA 证书内容。
    pub pem: String,
    /// CA 证书的指纹（SHA-256，用于前端显示/校验）。
    pub fingerprint: String,
    /// CA 的 Common Name。
    pub common_name: String,
}

/// `start_proxy()` 的请求体。
///
/// 参数显式包装为 DTO：`#[rpc_method]` 代码生成链只识别
/// DTO（大写开头的自定义类型）作为 body，裸基础类型（u16 等）
/// 会导致生成的 TS 客户端与真实路由契约不一致。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct StartProxyInput {
    /// 监听端口（缺省/0 = 操作系统分配空闲端口）
    pub port: Option<u16>,
}
