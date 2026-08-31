# cuckoo-dto

> 前端可见的 DTO（Data Transfer Object）类型层 —— API 契约的唯一来源。

## 功能

所有通过 `ts-rs` 导出给前端的 TypeScript 类型都定义在此处，而非 Entity 上。

### 定义的类型

| 类别 | 类型 |
|---|---|
| 数据模型 | `WorkspaceDto` → `WorkspaceModel`、`FolderDto` → `FolderModel`、`HttpRequestDefDto` → `HttpRequestDefModel`、`EnvironmentDto` → `EnvironmentModel` |
| 创建/更新输入 | `CreateWorkspaceInput` / `UpdateWorkspaceInput` / `CreateFolderInput` / `UpdateFolderInput` / `CreateRequestInput` / `UpdateRequestInput` / `CreateEnvironmentInput` / `UpdateEnvironmentInput` |
| 请求执行 | `AdHocRequest` / `SendRequestInput` / `ExecuteRequestInput` / `ExecutionResult` |
| 通用 | `PongResponse` / `HeaderEntry` / `KeyValueEntry` / `WorkspaceSettings` / `RequestBody` / `AuthConfig` / `EnvVariable` |

### 设计原则

1. **Entity 不导出 TS 类型**：`cuckoo-store` 的 Model 上不再有 `#[ts(export)]`，数据库结构变更不会直接影响 API 契约。
2. **DTO 字段使用强类型**：Entity 中的 Json 列在 DTO 中映射为具体的 `Vec<HeaderEntry>` 等类型，而非 `serde_json::Value`。
3. **转换逻辑集中**：`From<Entity>` 实现统一在 `convert.rs` 中，Service 层只需 `.into()` 即可完成转换。

## 目录结构

```
src/
├── lib.rs       # 模块入口，重导出 types / convert
├── types.rs     # 所有 DTO 类型定义（#[derive(ts_rs::TS)]）
└── convert.rs   # Entity ↔ DTO 转换实现（From<Entity> for Dto / From<Dto> for Entity）
```

## 层次关系

```
Frontend (TypeScript) <-- ts-rs export --> cuckoo-dto -- depends on --> cuckoo-store (Entity / SeaORM)
```

## 依赖关系

- 被 `cuckoo-service`、`cuckoo-http` 依赖
- 依赖 `cuckoo-store`（Entity 类型）、`ts-rs`（TS 类型导出）、`serde`
