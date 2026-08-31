//! Workspace CRUD 操作。
//!
//! 本模块只负责数据库读写，返回 Entity Model。
//! Input 参数使用原始类型（String, serde_json::Value 等），
//! 由 Service 层负责从 DTO 转换。

use sea_orm::*;

use crate::entities::workspace::{
    ActiveModel as WorkspaceActiveModel, Entity as WorkspaceEntity, Model as WorkspaceModel,
    WorkspaceSettings,
};

/// 创建 Workspace 的原始参数（repo 层使用）。
pub struct CreateWorkspaceParams {
    pub name: String,
    pub base_headers: serde_json::Value,
    pub settings: serde_json::Value,
}

/// 更新 Workspace 的原始参数（repo 层使用）。
pub struct UpdateWorkspaceParams {
    pub name: Option<String>,
    pub base_headers: Option<serde_json::Value>,
    pub settings: Option<serde_json::Value>,
}

pub async fn create(
    db: &DatabaseConnection,
    params: CreateWorkspaceParams,
) -> Result<WorkspaceModel, DbErr> {
    let now = chrono::Utc::now().timestamp_millis();
    let id = ulid::Ulid::new().to_string();

    let model = WorkspaceActiveModel {
        id: sea_orm::ActiveValue::Set(id),
        name: sea_orm::ActiveValue::Set(params.name),
        base_headers: sea_orm::ActiveValue::Set(params.base_headers),
        settings: sea_orm::ActiveValue::Set(params.settings),
        created_at: sea_orm::ActiveValue::Set(now),
        updated_at: sea_orm::ActiveValue::Set(now),
    };

    let result = model.insert(db).await?;
    WorkspaceEntity::find_by_id(result.id).one(db).await?.ok_or(DbErr::RecordNotFound("workspace".into()))
}

pub async fn find_by_id(
    db: &DatabaseConnection,
    id: &str,
) -> Result<Option<WorkspaceModel>, DbErr> {
    WorkspaceEntity::find_by_id(id.to_string()).one(db).await
}

pub async fn find_all(db: &DatabaseConnection) -> Result<Vec<WorkspaceModel>, DbErr> {
    WorkspaceEntity::find().all(db).await
}

pub async fn update(
    db: &DatabaseConnection,
    id: &str,
    params: UpdateWorkspaceParams,
) -> Result<WorkspaceModel, DbErr> {
    let existing = WorkspaceEntity::find_by_id(id.to_string())
        .one(db)
        .await?
        .ok_or(DbErr::RecordNotFound("workspace".into()))?;

    let now = chrono::Utc::now().timestamp_millis();
    let mut model: WorkspaceActiveModel = existing.into();

    if let Some(name) = params.name {
        model.name = sea_orm::ActiveValue::Set(name);
    }
    if let Some(base_headers) = params.base_headers {
        model.base_headers = sea_orm::ActiveValue::Set(base_headers);
    }
    if let Some(settings) = params.settings {
        model.settings = sea_orm::ActiveValue::Set(settings);
    }
    model.updated_at = sea_orm::ActiveValue::Set(now);

    let result = model.update(db).await?;
    Ok(result)
}

pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), DbErr> {
    WorkspaceEntity::delete_by_id(id.to_string())
        .exec(db)
        .await?;
    Ok(())
}

/// 获取 Workspace 的默认创建参数。
pub fn default_create_params(name: &str) -> CreateWorkspaceParams {
    CreateWorkspaceParams {
        name: name.to_string(),
        base_headers: serde_json::json!([]),
        settings: serde_json::json!(WorkspaceSettings::default()),
    }
}
