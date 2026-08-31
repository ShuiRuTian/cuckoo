//! Folder CRUD 操作。
//!
//! 本模块只负责数据库读写，返回 Entity Model。
//! Input 参数使用原始类型，由 Service 层负责从 DTO 转换。

use sea_orm::*;

use crate::entities::folder::{
    ActiveModel as FolderActiveModel, Entity as FolderEntity, Model as FolderModel,
};

/// 创建 Folder 的原始参数（repo 层使用）。
pub struct CreateFolderParams {
    pub workspace_id: String,
    pub parent_folder_id: Option<String>,
    pub name: String,
}

/// 更新 Folder 的原始参数（repo 层使用）。
pub struct UpdateFolderParams {
    pub name: Option<String>,
    pub parent_folder_id: Option<Option<String>>,
    pub sort_key: Option<f64>,
}

pub async fn create(db: &DatabaseConnection, params: CreateFolderParams) -> Result<FolderModel, DbErr> {
    let id = ulid::Ulid::new().to_string();
    let model = FolderActiveModel {
        id: sea_orm::ActiveValue::Set(id),
        workspace_id: sea_orm::ActiveValue::Set(params.workspace_id),
        parent_folder_id: sea_orm::ActiveValue::Set(params.parent_folder_id),
        name: sea_orm::ActiveValue::Set(params.name),
        sort_key: sea_orm::ActiveValue::Set(0.0),
    };

    let result = model.insert(db).await?;
    FolderEntity::find_by_id(result.id).one(db).await?.ok_or(DbErr::RecordNotFound("folder".into()))
}

pub async fn find_by_id(db: &DatabaseConnection, id: &str) -> Result<Option<FolderModel>, DbErr> {
    FolderEntity::find_by_id(id.to_string()).one(db).await
}

pub async fn find_by_workspace(
    db: &DatabaseConnection,
    workspace_id: &str,
) -> Result<Vec<FolderModel>, DbErr> {
    FolderEntity::find()
        .filter(crate::entities::folder::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await
}

pub async fn update(
    db: &DatabaseConnection,
    id: &str,
    params: UpdateFolderParams,
) -> Result<FolderModel, DbErr> {
    let existing = FolderEntity::find_by_id(id.to_string())
        .one(db)
        .await?
        .ok_or(DbErr::RecordNotFound("folder".into()))?;

    let mut model: FolderActiveModel = existing.into();

    if let Some(name) = params.name {
        model.name = sea_orm::ActiveValue::Set(name);
    }
    if let Some(parent_folder_id) = params.parent_folder_id {
        model.parent_folder_id = sea_orm::ActiveValue::Set(parent_folder_id);
    }
    if let Some(sort_key) = params.sort_key {
        model.sort_key = sea_orm::ActiveValue::Set(sort_key);
    }

    let result = model.update(db).await?;
    Ok(result)
}

pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), DbErr> {
    FolderEntity::delete_by_id(id.to_string()).exec(db).await?;
    Ok(())
}
