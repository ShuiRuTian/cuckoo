//! `collection_service`：Workspace/Folder/Request/Environment 的增删改查方法
//! （`spec.md` 7.2 节，`plan.md` M1.3 节）。
//!
//! 每个方法标注 `#[rpc_method]` 暴露为 REST 端点。
//! 返回 DTO 类型（非 Entity Model），接受 DTO Input 类型。

use cuckoo_core::{ServiceError, ServiceResult};
use cuckoo_dto::{
    CreateEnvironmentInput, CreateFolderInput, CreateRequestInput, CreateWorkspaceInput,
    EnvironmentDto, FolderDto, HttpRequestDefDto, UpdateEnvironmentInput, UpdateFolderInput,
    UpdateRequestInput, UpdateWorkspaceInput, WorkspaceDto,
};
use cuckoo_macros::rpc_method;
use cuckoo_store::repo::{
    environment_repo, folder_repo, request_repo, workspace_repo,
    workspace_repo::{CreateWorkspaceParams, UpdateWorkspaceParams},
    folder_repo::{CreateFolderParams, UpdateFolderParams},
    request_repo::{CreateRequestParams, UpdateRequestParams},
    environment_repo::{CreateEnvironmentParams, UpdateEnvironmentParams},
};
use sea_orm::DatabaseConnection;

// ─── Workspace ───

#[rpc_method("POST", "/api/workspaces")]
pub async fn create_workspace(
    db: &DatabaseConnection,
    input: CreateWorkspaceInput,
) -> ServiceResult<WorkspaceDto> {
    let params = CreateWorkspaceParams {
        base_headers: input.base_headers_json(),
        settings: input.settings_json(),
        name: input.name,
    };
    workspace_repo::create(db, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("GET", "/api/workspaces")]
pub async fn list_workspaces(db: &DatabaseConnection) -> ServiceResult<Vec<WorkspaceDto>> {
    workspace_repo::find_all(db)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(|v| v.into_iter().map(Into::into).collect())
}

#[rpc_method("GET", "/api/workspaces/{id}")]
pub async fn get_workspace(db: &DatabaseConnection, id: String) -> ServiceResult<WorkspaceDto> {
    workspace_repo::find_by_id(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
        .ok_or_else(|| ServiceError::NotFound(format!("workspace {id}")))
        .map(Into::into)
}

#[rpc_method("PUT", "/api/workspaces/{id}")]
pub async fn update_workspace(
    db: &DatabaseConnection,
    id: String,
    input: UpdateWorkspaceInput,
) -> ServiceResult<WorkspaceDto> {
    let params = UpdateWorkspaceParams {
        base_headers: input.base_headers_json(),
        settings: input.settings_json(),
        name: input.name,
    };
    workspace_repo::update(db, &id, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("DELETE", "/api/workspaces/{id}")]
pub async fn delete_workspace(db: &DatabaseConnection, id: String) -> ServiceResult<()> {
    workspace_repo::delete(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
}

// ─── Folder ───

#[rpc_method("POST", "/api/folders")]
pub async fn create_folder(
    db: &DatabaseConnection,
    input: CreateFolderInput,
) -> ServiceResult<FolderDto> {
    let params = CreateFolderParams {
        workspace_id: input.workspace_id,
        parent_folder_id: input.parent_folder_id,
        name: input.name,
    };
    folder_repo::create(db, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("GET", "/api/folders/workspace/{workspace_id}")]
pub async fn list_folders(
    db: &DatabaseConnection,
    workspace_id: String,
) -> ServiceResult<Vec<FolderDto>> {
    folder_repo::find_by_workspace(db, &workspace_id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(|v| v.into_iter().map(Into::into).collect())
}

#[rpc_method("PUT", "/api/folders/{id}")]
pub async fn update_folder(
    db: &DatabaseConnection,
    id: String,
    input: UpdateFolderInput,
) -> ServiceResult<FolderDto> {
    let params = UpdateFolderParams {
        name: input.name,
        parent_folder_id: input.parent_folder_id,
        sort_key: input.sort_key,
    };
    folder_repo::update(db, &id, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("DELETE", "/api/folders/{id}")]
pub async fn delete_folder(db: &DatabaseConnection, id: String) -> ServiceResult<()> {
    folder_repo::delete(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
}

// ─── Request ───

#[rpc_method("POST", "/api/requests")]
pub async fn create_request(
    db: &DatabaseConnection,
    input: CreateRequestInput,
) -> ServiceResult<HttpRequestDefDto> {
    let params = CreateRequestParams {
        headers: input.headers_json(),
        query_params: input.query_params_json(),
        body: input.body_json(),
        auth: input.auth_json(),
        workspace_id: input.workspace_id,
        folder_id: input.folder_id,
        name: input.name,
        method: input.method,
        url: input.url,
    };
    request_repo::create(db, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("GET", "/api/requests/workspace/{workspace_id}")]
pub async fn list_requests(
    db: &DatabaseConnection,
    workspace_id: String,
) -> ServiceResult<Vec<HttpRequestDefDto>> {
    request_repo::find_by_workspace(db, &workspace_id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(|v| v.into_iter().map(Into::into).collect())
}

#[rpc_method("GET", "/api/requests/{id}")]
pub async fn get_request(db: &DatabaseConnection, id: String) -> ServiceResult<HttpRequestDefDto> {
    request_repo::find_by_id(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))?
        .ok_or_else(|| ServiceError::NotFound(format!("request {id}")))
        .map(Into::into)
}

#[rpc_method("PUT", "/api/requests/{id}")]
pub async fn update_request(
    db: &DatabaseConnection,
    id: String,
    input: UpdateRequestInput,
) -> ServiceResult<HttpRequestDefDto> {
    let params = UpdateRequestParams {
        headers: input.headers_json(),
        query_params: input.query_params_json(),
        body: input.body_json(),
        auth: input.auth_json(),
        folder_id: input.folder_id,
        name: input.name,
        method: input.method,
        url: input.url,
        sort_key: input.sort_key,
    };
    request_repo::update(db, &id, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("DELETE", "/api/requests/{id}")]
pub async fn delete_request(db: &DatabaseConnection, id: String) -> ServiceResult<()> {
    request_repo::delete(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
}

// ─── Environment ───

#[rpc_method("POST", "/api/environments")]
pub async fn create_environment(
    db: &DatabaseConnection,
    input: CreateEnvironmentInput,
) -> ServiceResult<EnvironmentDto> {
    let params = CreateEnvironmentParams {
        variables: input.variables_json(),
        workspace_id: input.workspace_id,
        name: input.name,
    };
    environment_repo::create(db, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("GET", "/api/environments/workspace/{workspace_id}")]
pub async fn list_environments(
    db: &DatabaseConnection,
    workspace_id: String,
) -> ServiceResult<Vec<EnvironmentDto>> {
    environment_repo::find_by_workspace(db, &workspace_id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(|v| v.into_iter().map(Into::into).collect())
}

#[rpc_method("PUT", "/api/environments/{id}")]
pub async fn update_environment(
    db: &DatabaseConnection,
    id: String,
    input: UpdateEnvironmentInput,
) -> ServiceResult<EnvironmentDto> {
    let params = UpdateEnvironmentParams {
        variables: input.variables_json(),
        name: input.name,
    };
    environment_repo::update(db, &id, params)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
        .map(Into::into)
}

#[rpc_method("DELETE", "/api/environments/{id}")]
pub async fn delete_environment(db: &DatabaseConnection, id: String) -> ServiceResult<()> {
    environment_repo::delete(db, &id)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
}
