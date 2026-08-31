//! Entity → DTO 的转换实现。
//!
//! 每个 `From<Entity::Model>` 实现将数据库 Model 转换为前端 DTO，
//! 包括将 `Json` 列反序列化为强类型字段。

use crate::types::*;

use cuckoo_store::entities::{
    environment::Model as EnvModel,
    folder::Model as FolderModel,
    http_request_def::Model as ReqModel,
    workspace::Model as WorkspaceModel,
};

// ─── Workspace ───

impl From<WorkspaceModel> for WorkspaceDto {
    fn from(m: WorkspaceModel) -> Self {
        WorkspaceDto {
            id: m.id,
            name: m.name,
            base_headers: serde_json::from_value(m.base_headers).unwrap_or_default(),
            settings: serde_json::from_value(m.settings).unwrap_or_default(),
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

// ─── Folder ───

impl From<FolderModel> for FolderDto {
    fn from(m: FolderModel) -> Self {
        FolderDto {
            id: m.id,
            workspace_id: m.workspace_id,
            parent_folder_id: m.parent_folder_id,
            name: m.name,
            sort_key: m.sort_key,
        }
    }
}

// ─── HttpRequestDef ───

impl From<ReqModel> for HttpRequestDefDto {
    fn from(m: ReqModel) -> Self {
        HttpRequestDefDto {
            id: m.id,
            folder_id: m.folder_id,
            workspace_id: m.workspace_id,
            name: m.name,
            method: m.method,
            url: m.url,
            headers: serde_json::from_value(m.headers).unwrap_or_default(),
            query_params: serde_json::from_value(m.query_params).unwrap_or_default(),
            body: serde_json::from_value(m.body).unwrap_or_default(),
            auth: serde_json::from_value(m.auth).unwrap_or_default(),
            pre_request_script: m.pre_request_script,
            post_response_script: m.post_response_script,
            sort_key: m.sort_key,
        }
    }
}

// ─── Environment ───

impl From<EnvModel> for EnvironmentDto {
    fn from(m: EnvModel) -> Self {
        EnvironmentDto {
            id: m.id,
            workspace_id: m.workspace_id,
            name: m.name,
            variables: serde_json::from_value(m.variables).unwrap_or_default(),
        }
    }
}

// ─── Input DTO → serde_json::Value 转换辅助 ───

impl CreateWorkspaceInput {
    /// 转换为 SeaORM ActiveModel 需要的 JSON Value。
    pub fn base_headers_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.base_headers).unwrap_or_default()
    }
    pub fn settings_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }
}

impl UpdateWorkspaceInput {
    pub fn base_headers_json(&self) -> Option<serde_json::Value> {
        self.base_headers
            .as_ref()
            .map(|v| serde_json::to_value(v).unwrap_or_default())
    }
    pub fn settings_json(&self) -> Option<serde_json::Value> {
        self.settings
            .as_ref()
            .map(|v| serde_json::to_value(v).unwrap_or_default())
    }
}

impl CreateRequestInput {
    pub fn headers_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.headers).unwrap_or_default()
    }
    pub fn query_params_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.query_params).unwrap_or_default()
    }
    pub fn body_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.body).unwrap_or_default()
    }
    pub fn auth_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.auth).unwrap_or_default()
    }
}

impl UpdateRequestInput {
    pub fn headers_json(&self) -> Option<serde_json::Value> {
        self.headers.as_ref().map(|v| serde_json::to_value(v).unwrap_or_default())
    }
    pub fn query_params_json(&self) -> Option<serde_json::Value> {
        self.query_params.as_ref().map(|v| serde_json::to_value(v).unwrap_or_default())
    }
    pub fn body_json(&self) -> Option<serde_json::Value> {
        self.body.as_ref().map(|v| serde_json::to_value(v).unwrap_or_default())
    }
    pub fn auth_json(&self) -> Option<serde_json::Value> {
        self.auth.as_ref().map(|v| serde_json::to_value(v).unwrap_or_default())
    }
}

impl CreateEnvironmentInput {
    pub fn variables_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.variables).unwrap_or_default()
    }
}

impl UpdateEnvironmentInput {
    pub fn variables_json(&self) -> Option<serde_json::Value> {
        self.variables.as_ref().map(|v| serde_json::to_value(v).unwrap_or_default())
    }
}

impl AdHocRequest {
    pub fn headers_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.headers).unwrap_or_default()
    }
    pub fn query_params_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.query_params).unwrap_or_default()
    }
    pub fn body_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.body).unwrap_or_default()
    }
    pub fn auth_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.auth).unwrap_or_default()
    }
}

impl ExecuteRequestInput {
    pub fn headers_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.headers).unwrap_or_default()
    }
    pub fn query_params_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.query_params).unwrap_or_default()
    }
    pub fn body_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.body).unwrap_or_default()
    }
    pub fn auth_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.auth).unwrap_or_default()
    }
}
