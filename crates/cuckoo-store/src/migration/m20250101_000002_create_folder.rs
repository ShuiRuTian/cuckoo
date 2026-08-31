use sea_orm_migration::prelude::*;

use crate::migration::m20250101_000001_create_workspace;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Folder 表的 Iden，用于跨迁移引用。
/// `Table` 变体使用 `#[sea_orm(iden = "folder")]` 确保引用正确的表名。
#[derive(DeriveIden)]
pub enum FolderIden {
    #[sea_orm(iden = "folder")]
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Folder::Table)
                    .col(
                        ColumnDef::new(Folder::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Folder::WorkspaceId).string().not_null())
                    .col(ColumnDef::new(Folder::ParentFolderId).string())
                    .col(ColumnDef::new(Folder::Name).string().not_null())
                    .col(ColumnDef::new(Folder::SortKey).double().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_folder_workspace")
                            .from(Folder::Table, Folder::WorkspaceId)
                            .to(
                                m20250101_000001_create_workspace::Workspace::Table,
                                m20250101_000001_create_workspace::Workspace::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_folder_parent")
                            .from(Folder::Table, Folder::ParentFolderId)
                            .to(Folder::Table, Folder::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Folder::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Folder {
    Table,
    Id,
    WorkspaceId,
    ParentFolderId,
    Name,
    SortKey,
}
