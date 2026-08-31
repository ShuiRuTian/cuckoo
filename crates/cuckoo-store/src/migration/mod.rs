//! `sea-orm-migration` 版本化迁移（`spec.md` 3.2 节）。
//!
//! 应用启动时自动执行待应用的迁移。

pub mod m20250101_000001_create_workspace;
pub mod m20250101_000002_create_folder;
pub mod m20250101_000003_create_http_request_def;
pub mod m20250101_000004_create_environment;

use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250101_000001_create_workspace::Migration),
            Box::new(m20250101_000002_create_folder::Migration),
            Box::new(m20250101_000003_create_http_request_def::Migration),
            Box::new(m20250101_000004_create_environment::Migration),
        ]
    }
}
