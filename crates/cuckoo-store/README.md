# cuckoo-store

> SQLite 存储层（SeaORM + 迁移管理）。

## 功能

- **数据库连接**：`connect()` 建立 SQLite 连接，开启 WAL 模式，自动执行迁移。
- **Entity 定义**：Workspace / Folder / HttpRequestDef / Environment 四个 SeaORM Entity 及其 `Related` 关联关系。
- **版本化迁移**：通过 `sea-orm-migration` 管理表结构创建与演进。
- **Repository 函数**：纯数据层 CRUD 函数（不含 `#[rpc_method]` 包装，那是 `cuckoo-service` 的职责）。

## 目录结构

```
src/
├── lib.rs           # connect() / default_db_path() / 模块入口
├── entities/        # SeaORM Entity 定义
│   ├── mod.rs
│   ├── workspace.rs       # Workspace Entity
│   ├── folder.rs          # Folder Entity（关联 Workspace）
│   ├── http_request_def.rs # HttpRequestDef Entity（关联 Folder）
│   └── environment.rs     # Environment Entity（关联 Workspace）
├── migration/       # 版本化迁移文件
│   ├── mod.rs
│   ├── m20250101_000001_create_workspace.rs
│   ├── m20250101_000002_create_folder.rs
│   ├── m20250101_000003_create_http_request_def.rs
│   └── m20250101_000004_create_environment.rs
└── repo/            # Repository 函数（纯数据层 CRUD）
    ├── mod.rs
    ├── workspace_repo.rs
    ├── folder_repo.rs
    ├── request_repo.rs
    └── environment_repo.rs
```

## 依赖关系

- 被 `cuckoo-dto`、`cuckoo-service`、`cuckoo-http`、`cuckoo-templates` 依赖
- 依赖 `sea-orm`、`sea-orm-migration`、`dirs`
- 数据库路径默认为 `~/Library/Application Support/Cuckoo/cuckoo.db`（macOS）
