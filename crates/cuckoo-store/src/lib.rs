//! `cuckoo-store`：SQLite 存储层（`spec.md` 3.2 节）。
//!
//! 接入 `sea-orm`（`sqlx-sqlite` 驱动 + `runtime-tokio-rustls`），
//! 用 `DeriveEntityModel` 定义 Workspace/Folder/HttpRequestDef/Environment 四个 Entity 及其
//! `Related` 关联关系。表结构的创建与演进通过 `sea-orm-migration` 的版本化迁移文件管理。
//!
//! 本 crate 只提供纯数据层逻辑（CRUD 函数），不包含 `#[rpc_method]` 包装——
//! 那是 `cuckoo-service` 的职责。

pub mod entities;
pub mod migration;
pub mod repo;

use sea_orm::{ConnectOptions, ConnectionTrait, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use std::time::Duration;

/// 建立到 SQLite 数据库的连接，开启 WAL 模式并自动执行迁移。
///
/// `db_path` 应为应用数据目录下的路径（如 `~/Library/Application Support/Cuckoo/cuckoo.db`）。
pub async fn connect(db_path: &str) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let url = format!("sqlite://{db_path}?mode=rwc");

    let mut opt = ConnectOptions::new(&url);
    opt.max_connections(5)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(300));

    let db = sea_orm::Database::connect(opt).await?;

    // 开启 WAL 模式（`investigation.md` 3.5 节）。
    db.execute_unprepared("PRAGMA journal_mode=WAL")
        .await?;

    // 执行待应用的迁移。
    migration::Migrator::up(&db, None).await?;

    Ok(db)
}

/// 便捷函数：返回应用数据目录下的默认数据库路径。
pub fn default_db_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Cuckoo");
    std::fs::create_dir_all(&dir).ok();
    dir.join("cuckoo.db")
}
