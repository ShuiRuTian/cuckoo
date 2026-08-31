use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Workspace::Table)
                    .col(
                        ColumnDef::new(Workspace::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Workspace::Name).string().not_null())
                    .col(
                        ColumnDef::new(Workspace::BaseHeaders)
                            .json()
                            .not_null()
                            .default(r#"[]"#),
                    )
                    .col(
                        ColumnDef::new(Workspace::Settings)
                            .json()
                            .not_null()
                            .default(r#"{"verify_tls":true,"timeout_ms":null}"#),
                    )
                    .col(ColumnDef::new(Workspace::CreatedAt).big_integer().not_null())
                    .col(ColumnDef::new(Workspace::UpdatedAt).big_integer().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Workspace::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Workspace {
    Table,
    Id,
    Name,
    BaseHeaders,
    Settings,
    CreatedAt,
    UpdatedAt,
}
