//! Environment CRUD 操作。
//!
//! 本模块只负责数据库读写，返回 Entity Model。
//! Input 参数使用原始类型，由 Service 层负责从 DTO 转换。

use sea_orm::*;

use crate::entities::environment::{
    ActiveModel as EnvActiveModel, Entity as EnvEntity, Model as EnvModel,
};

/// 创建 Environment 的原始参数（repo 层使用）。
pub struct CreateEnvironmentParams {
    pub workspace_id: String,
    pub name: String,
    pub variables: serde_json::Value,
}

/// 更新 Environment 的原始参数（repo 层使用）。
pub struct UpdateEnvironmentParams {
    pub name: Option<String>,
    pub variables: Option<serde_json::Value>,
}

pub async fn create(
    db: &DatabaseConnection,
    params: CreateEnvironmentParams,
) -> Result<EnvModel, DbErr> {
    let id = ulid::Ulid::new().to_string();
    let model = EnvActiveModel {
        id: sea_orm::ActiveValue::Set(id),
        workspace_id: sea_orm::ActiveValue::Set(params.workspace_id),
        name: sea_orm::ActiveValue::Set(params.name),
        variables: sea_orm::ActiveValue::Set(params.variables),
    };

    let result = model.insert(db).await?;
    EnvEntity::find_by_id(result.id).one(db).await?.ok_or(DbErr::RecordNotFound("environment".into()))
}

pub async fn find_by_id(db: &DatabaseConnection, id: &str) -> Result<Option<EnvModel>, DbErr> {
    EnvEntity::find_by_id(id.to_string()).one(db).await
}

pub async fn find_by_workspace(
    db: &DatabaseConnection,
    workspace_id: &str,
) -> Result<Vec<EnvModel>, DbErr> {
    EnvEntity::find()
        .filter(crate::entities::environment::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await
}

pub async fn update(
    db: &DatabaseConnection,
    id: &str,
    params: UpdateEnvironmentParams,
) -> Result<EnvModel, DbErr> {
    let existing = EnvEntity::find_by_id(id.to_string())
        .one(db)
        .await?
        .ok_or(DbErr::RecordNotFound("environment".into()))?;

    let mut model: EnvActiveModel = existing.into();

    if let Some(name) = params.name {
        model.name = sea_orm::ActiveValue::Set(name);
    }
    if let Some(variables) = params.variables {
        model.variables = sea_orm::ActiveValue::Set(variables);
    }

    let result = model.update(db).await?;
    Ok(result)
}

pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), DbErr> {
    EnvEntity::delete_by_id(id.to_string()).exec(db).await?;
    Ok(())
}
