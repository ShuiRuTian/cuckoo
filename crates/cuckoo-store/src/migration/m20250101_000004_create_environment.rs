use sea_orm_migration::prelude::*;

use crate::migration::m20250101_000001_create_workspace;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Environment::Table)
                    .col(
                        ColumnDef::new(Environment::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Environment::WorkspaceId).string().not_null())
                    .col(ColumnDef::new(Environment::Name).string().not_null())
                    .col(
                        ColumnDef::new(Environment::Variables)
                            .json()
                            .not_null()
                            .default(r#"[]"#),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_env_workspace")
                            .from(Environment::Table, Environment::WorkspaceId)
                            .to(
                                m20250101_000001_create_workspace::Workspace::Table,
                                m20250101_000001_create_workspace::Workspace::Id,
                            )
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Environment::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Environment {
    Table,
    Id,
    WorkspaceId,
    Name,
    Variables,
}
