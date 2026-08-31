//! Collection CRUD 路由：Workspace/Folder/Request/Environment 的增删改查。
//!
//! 对应 `cuckoo_service::collection_service` 中的方法（`plan.md` M1.3 节）。
//! 路由参数通过 axum `Path` / `Json` 提取器从 URL 和 body 中解析，
//! `DatabaseConnection` 从 `AuthState.db` 获取后传入 Service 层方法。
//!
//! 所有 handler 的入参和返回值均使用 `cuckoo-dto` 中的 DTO 类型。

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_dto::{
    CreateEnvironmentInput, CreateFolderInput, CreateRequestInput, CreateWorkspaceInput,
    EnvironmentDto, FolderDto, HttpRequestDefDto, UpdateEnvironmentInput, UpdateFolderInput,
    UpdateRequestInput, UpdateWorkspaceInput, WorkspaceDto,
};
use cuckoo_service::collection_service;

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new()
        // ─── Workspace ───
        .route("/api/workspaces", post(create_workspace))
        .route("/api/workspaces", get(list_workspaces))
        .route("/api/workspaces/{id}", get(get_workspace))
        .route("/api/workspaces/{id}", put(update_workspace))
        .route("/api/workspaces/{id}", delete(delete_workspace))
        // ─── Folder ───
        .route("/api/folders", post(create_folder))
        .route("/api/folders/workspace/{workspace_id}", get(list_folders))
        .route("/api/folders/{id}", put(update_folder))
        .route("/api/folders/{id}", delete(delete_folder))
        // ─── Request ───
        .route("/api/requests", post(create_request))
        .route("/api/requests/workspace/{workspace_id}", get(list_requests))
        .route("/api/requests/{id}", get(get_request))
        .route("/api/requests/{id}", put(update_request))
        .route("/api/requests/{id}", delete(delete_request))
        // ─── Environment ───
        .route("/api/environments", post(create_environment))
        .route(
            "/api/environments/workspace/{workspace_id}",
            get(list_environments),
        )
        .route("/api/environments/{id}", put(update_environment))
        .route("/api/environments/{id}", delete(delete_environment))
}

// ─── Workspace handlers ───

async fn create_workspace(
    State(state): State<AuthState>,
    Json(input): Json<CreateWorkspaceInput>,
) -> Result<Json<WorkspaceDto>, ServiceError> {
    let result = collection_service::create_workspace(&state.db, input).await?;
    Ok(Json(result))
}

async fn list_workspaces(
    State(state): State<AuthState>,
) -> Result<Json<Vec<WorkspaceDto>>, ServiceError> {
    let result = collection_service::list_workspaces(&state.db).await?;
    Ok(Json(result))
}

async fn get_workspace(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceDto>, ServiceError> {
    let result = collection_service::get_workspace(&state.db, id).await?;
    Ok(Json(result))
}

async fn update_workspace(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateWorkspaceInput>,
) -> Result<Json<WorkspaceDto>, ServiceError> {
    let result = collection_service::update_workspace(&state.db, id, input).await?;
    Ok(Json(result))
}

async fn delete_workspace(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServiceError> {
    collection_service::delete_workspace(&state.db, id).await?;
    Ok(Json(()))
}

// ─── Folder handlers ───

async fn create_folder(
    State(state): State<AuthState>,
    Json(input): Json<CreateFolderInput>,
) -> Result<Json<FolderDto>, ServiceError> {
    let result = collection_service::create_folder(&state.db, input).await?;
    Ok(Json(result))
}

async fn list_folders(
    State(state): State<AuthState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<FolderDto>>, ServiceError> {
    let result = collection_service::list_folders(&state.db, workspace_id).await?;
    Ok(Json(result))
}

async fn update_folder(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateFolderInput>,
) -> Result<Json<FolderDto>, ServiceError> {
    let result = collection_service::update_folder(&state.db, id, input).await?;
    Ok(Json(result))
}

async fn delete_folder(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServiceError> {
    collection_service::delete_folder(&state.db, id).await?;
    Ok(Json(()))
}

// ─── Request handlers ───

async fn create_request(
    State(state): State<AuthState>,
    Json(input): Json<CreateRequestInput>,
) -> Result<Json<HttpRequestDefDto>, ServiceError> {
    let result = collection_service::create_request(&state.db, input).await?;
    Ok(Json(result))
}

async fn list_requests(
    State(state): State<AuthState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<HttpRequestDefDto>>, ServiceError> {
    let result = collection_service::list_requests(&state.db, workspace_id).await?;
    Ok(Json(result))
}

async fn get_request(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<HttpRequestDefDto>, ServiceError> {
    let result = collection_service::get_request(&state.db, id).await?;
    Ok(Json(result))
}

async fn update_request(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateRequestInput>,
) -> Result<Json<HttpRequestDefDto>, ServiceError> {
    let result = collection_service::update_request(&state.db, id, input).await?;
    Ok(Json(result))
}

async fn delete_request(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServiceError> {
    collection_service::delete_request(&state.db, id).await?;
    Ok(Json(()))
}

// ─── Environment handlers ───

async fn create_environment(
    State(state): State<AuthState>,
    Json(input): Json<CreateEnvironmentInput>,
) -> Result<Json<EnvironmentDto>, ServiceError> {
    let result = collection_service::create_environment(&state.db, input).await?;
    Ok(Json(result))
}

async fn list_environments(
    State(state): State<AuthState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<Vec<EnvironmentDto>>, ServiceError> {
    let result = collection_service::list_environments(&state.db, workspace_id).await?;
    Ok(Json(result))
}

async fn update_environment(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateEnvironmentInput>,
) -> Result<Json<EnvironmentDto>, ServiceError> {
    let result = collection_service::update_environment(&state.db, id, input).await?;
    Ok(Json(result))
}

async fn delete_environment(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<()>, ServiceError> {
    collection_service::delete_environment(&state.db, id).await?;
    Ok(Json(()))
}
