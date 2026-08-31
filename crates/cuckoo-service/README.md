# cuckoo-service

> ★ Service 层 —— 唯一包含业务逻辑的地方。

所有业务方法标注 `#[rpc_method]` 后自动暴露为 REST 端点，函数签名不出现任何 Tauri 或其他传输层专属类型。

## 功能

### 已实现模块

| 模块 | 功能 | 端点示例 |
|---|---|---|
| `ping_service` | 端到端闭环验证 | `GET /api/ping` |
| `collection_service` | Workspace / Folder / Request / Environment CRUD | `POST /api/workspaces`、`GET /api/folders/:id` 等 |
| `request_service` | 发送 HTTP 请求（按 ID 或 ad-hoc） | `POST /api/requests/send` |

### 计划模块（按 plan.md 逐步补齐）

| 模块 | 功能 | 阶段 |
|---|---|---|
| `proxy_service` | `start_proxy()` / `stop_proxy()` / Flow 订阅 | M2 |
| `rule_service` | 拦截规则 CRUD、`resume_intercept()` | M3 |
| `system_service` | 证书导出、系统代理设置 | M2 |

## 目录结构

```
src/
├── lib.rs                 # 模块入口，声明子模块 + 重导出 ping / send_request
├── ping_service.rs        # ping() 方法
├── collection_service.rs  # Workspace / Folder / Request / Environment CRUD
├── request_service.rs     # send_request() 方法
├── proxy_service.rs       # ProxyState：start_proxy / stop_proxy / Flow 订阅管理
└── system_service.rs      # 系统管理方法（证书导出等）
```

## 设计原则

- 所有 Service 方法的返回值和入参均使用 `cuckoo-dto` 中定义的 DTO 类型，不直接暴露 Entity Model。
- `#[rpc_method("METHOD", "/api/path")]` 宏自动：
  1. 将路由元信息写入 `.rpc_routes.json`，供 `build.rs` 生成前端 TS 客户端和 CLI Rust 客户端。
  2. 通过 `inventory::submit!` 登记进全局清单，供 server 启动期路由表打印/自检。

## 依赖关系

- 被 `cuckoo-server`（调用 Service 方法 + 拼装 Router）、`cuckoo-desktop`（spawn server）依赖
- 依赖 `cuckoo-core`（ServiceError）、`cuckoo-store`（数据库 CRUD）、`cuckoo-dto`（DTO 类型）、`cuckoo-http`（请求执行器）、`cuckoo-templates`（变量渲染）、`cuckoo-macros`（#[rpc_method]）
