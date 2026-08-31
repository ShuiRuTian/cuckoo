# cuckoo-core

> Tauri 无关的公共类型、错误处理与 RPC 方法登记表。

整个 workspace 里唯一允许被几乎所有其他 crate 依赖的基础 crate，不包含任何具体业务逻辑。

## 功能

- **错误类型**：`ServiceError` / `ServiceResult`，所有 Service 方法与 HTTP handler 共用同一套错误类型，实现 `IntoResponse` 自动转换为 HTTP 响应。
- **RPC 登记表**：`RpcMethodDescriptor` + `inventory::collect!`，`#[rpc_method]` 宏在编译期将每个标注方法登记进全局清单，供 server 启动期路由表打印/自检使用。

## 目录结构

```
src/
├── lib.rs           # 模块入口，重导出 error / rpc_registry
├── error.rs         # ServiceError 枚举（NotFound/BadRequest/Unauthorized/Internal）+ IntoResponse
└── rpc_registry.rs  # RpcMethodDescriptor 结构体 + inventory 收集 + all_descriptors() + build_router()
```

## 依赖关系

- 被 `cuckoo-macros`、`cuckoo-service`、`cuckoo-server` 等几乎所有 crate 依赖
- 依赖 `axum`（IntoResponse）、`thiserror`、`inventory`、`serde`
