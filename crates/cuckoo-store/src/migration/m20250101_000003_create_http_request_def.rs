use sea_orm_migration::prelude::*;

use crate::migration::m20250101_000001_create_workspace;
use crate::migration::m20250101_000002_create_folder::FolderIden;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(HttpRequestDef::Table)
                    .col(
                        ColumnDef::new(HttpRequestDef::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(HttpRequestDef::FolderId).string())
                    .col(ColumnDef::new(HttpRequestDef::WorkspaceId).string().not_null())
                    .col(ColumnDef::new(HttpRequestDef::Name).string().not_null())
                    .col(ColumnDef::new(HttpRequestDef::Method).string().not_null())
                    .col(ColumnDef::new(HttpRequestDef::Url).string().not_null())
                    .col(
                        ColumnDef::new(HttpRequestDef::Headers)
                            .json()
                            .not_null()
                            .default(r#"[]"#),
                    )
                    .col(
                        ColumnDef::new(HttpRequestDef::QueryParams)
                            .json()
                            .not_null()
                            .default(r#"[]"#),
                    )
                    .col(
                        ColumnDef::new(HttpRequestDef::Body)
                            .json()
                            .not_null()
                            .default(r#"{"type":"none"}"#),
                    )
                    .col(
                        ColumnDef::new(HttpRequestDef::Auth)
                            .json()
                            .not_null()
                            .default(r#"{"type":"none"}"#),
                    )
                    .col(ColumnDef::new(HttpRequestDef::PreRequestScript).string())
                    .col(ColumnDef::new(HttpRequestDef::PostResponseScript).string())
                    .col(ColumnDef::new(HttpRequestDef::SortKey).double().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_request_workspace")
                            .from(HttpRequestDef::Table, HttpRequestDef::WorkspaceId)
                            .to(
                                m20250101_000001_create_workspace::Workspace::Table,
                                m20250101_000001_create_workspace::Workspace::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_request_folder")
                            .from(HttpRequestDef::Table, HttpRequestDef::FolderId)
                            .to(
                                FolderIden::Table,
                                FolderIden::Id,
                            )
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(HttpRequestDef::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum HttpRequestDef {
    Table,
    Id,
    FolderId,
    WorkspaceId,
    Name,
    Method,
    Url,
    Headers,
    QueryParams,
    Body,
    Auth,
    PreRequestScript,
    PostResponseScript,
    SortKey,
}
