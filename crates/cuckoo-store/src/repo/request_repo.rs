//! HttpRequestDef CRUD 操作。
//!
//! 本模块只负责数据库读写，返回 Entity Model。
//! Input 参数使用原始类型，由 Service 层负责从 DTO 转换。

use sea_orm::*;

use crate::entities::http_request_def::{
    ActiveModel as RequestActiveModel, Entity as RequestEntity, Model as RequestModel,
};

/// 创建 HTTP 请求定义的原始参数（repo 层使用）。
pub struct CreateRequestParams {
    pub workspace_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: serde_json::Value,
    pub query_params: serde_json::Value,
    pub body: serde_json::Value,
    pub auth: serde_json::Value,
}

/// 更新 HTTP 请求定义的原始参数（repo 层使用）。
pub struct UpdateRequestParams {
    pub folder_id: Option<Option<String>>,
    pub name: Option<String>,
    pub method: Option<String>,
    pub url: Option<String>,
    pub headers: Option<serde_json::Value>,
    pub query_params: Option<serde_json::Value>,
    pub body: Option<serde_json::Value>,
    pub auth: Option<serde_json::Value>,
    pub sort_key: Option<f64>,
}

pub async fn create(
    db: &DatabaseConnection,
    params: CreateRequestParams,
) -> Result<RequestModel, DbErr> {
    let id = ulid::Ulid::new().to_string();
    let model = RequestActiveModel {
        id: sea_orm::ActiveValue::Set(id),
        workspace_id: sea_orm::ActiveValue::Set(params.workspace_id),
        folder_id: sea_orm::ActiveValue::Set(params.folder_id),
        name: sea_orm::ActiveValue::Set(params.name),
        method: sea_orm::ActiveValue::Set(params.method),
        url: sea_orm::ActiveValue::Set(params.url),
        headers: sea_orm::ActiveValue::Set(params.headers),
        query_params: sea_orm::ActiveValue::Set(params.query_params),
        body: sea_orm::ActiveValue::Set(params.body),
        auth: sea_orm::ActiveValue::Set(params.auth),
        pre_request_script: sea_orm::ActiveValue::Set(None),
        post_response_script: sea_orm::ActiveValue::Set(None),
        sort_key: sea_orm::ActiveValue::Set(0.0),
    };

    let result = model.insert(db).await?;
    RequestEntity::find_by_id(result.id).one(db).await?.ok_or(DbErr::RecordNotFound("http_request_def".into()))
}

pub async fn find_by_id(db: &DatabaseConnection, id: &str) -> Result<Option<RequestModel>, DbErr> {
    RequestEntity::find_by_id(id.to_string()).one(db).await
}

pub async fn find_by_workspace(
    db: &DatabaseConnection,
    workspace_id: &str,
) -> Result<Vec<RequestModel>, DbErr> {
    RequestEntity::find()
        .filter(crate::entities::http_request_def::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await
}

pub async fn find_by_folder(
    db: &DatabaseConnection,
    folder_id: &str,
) -> Result<Vec<RequestModel>, DbErr> {
    RequestEntity::find()
        .filter(crate::entities::http_request_def::Column::FolderId.eq(folder_id))
        .all(db)
        .await
}

pub async fn update(
    db: &DatabaseConnection,
    id: &str,
    params: UpdateRequestParams,
) -> Result<RequestModel, DbErr> {
    let existing = RequestEntity::find_by_id(id.to_string())
        .one(db)
        .await?
        .ok_or(DbErr::RecordNotFound("http_request_def".into()))?;

    let mut model: RequestActiveModel = existing.into();

    if let Some(folder_id) = params.folder_id {
        model.folder_id = sea_orm::ActiveValue::Set(folder_id);
    }
    if let Some(name) = params.name {
        model.name = sea_orm::ActiveValue::Set(name);
    }
    if let Some(method) = params.method {
        model.method = sea_orm::ActiveValue::Set(method);
    }
    if let Some(url) = params.url {
        model.url = sea_orm::ActiveValue::Set(url);
    }
    if let Some(headers) = params.headers {
        model.headers = sea_orm::ActiveValue::Set(headers);
    }
    if let Some(query_params) = params.query_params {
        model.query_params = sea_orm::ActiveValue::Set(query_params);
    }
    if let Some(body) = params.body {
        model.body = sea_orm::ActiveValue::Set(body);
    }
    if let Some(auth) = params.auth {
        model.auth = sea_orm::ActiveValue::Set(auth);
    }
    if let Some(sort_key) = params.sort_key {
        model.sort_key = sea_orm::ActiveValue::Set(sort_key);
    }

    let result = model.update(db).await?;
    Ok(result)
}

pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), DbErr> {
    RequestEntity::delete_by_id(id.to_string()).exec(db).await?;
    Ok(())
}
