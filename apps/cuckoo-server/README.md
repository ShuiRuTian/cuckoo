# cuckoo-server

> 本地 HTTP+SSE Server（axum）—— 唯一的业务协议入口。

既可以由 `cuckoo-desktop` 在同进程内 `tokio::spawn` 内嵌拉起，也可以独立编译运行（`cuckoo-server --headless`），两种方式内部持有的都是同一个 Service 层实例，行为完全一致。

## 功能

- **业务 API**：通过 `#[rpc_method]` 宏自动生成 REST 路由，调用 `cuckoo-service` 的业务方法。
- **SSE 端点**：`GET /api/flows/stream`，订阅 Flow 事件并推送给连接的客户端（桌面 UI / CLI / MCP / 浏览器共用同一端点）。
- **鉴权**：`Authorization: Bearer <token>` 中间件，token 文件自动生成/读取。
- **CORS / Origin 校验**：放行 `tauri://localhost` 等 Tauri 页面源发起的跨源请求。
- **健康检查**：`GET /healthz`（不需要鉴权）。
- **Headless 发现**：启动时将端口写入 `server.port` 文件，供 `cuckoo-cli` 探测连接。
- **构建时代码生成**：`build.rs` 扫描 `#[rpc_method]` 清单，自动生成前端 TS 客户端和 CLI Rust 客户端。

## 目录结构

```
src/
├── lib.rs            # spawn_server() / build_app() / ServerHandle
├── main.rs           # 独立运行入口（--headless --port）
├── auth.rs           # AuthState / require_auth / validate_origin / load_or_create_token / cors_layer
├── sse.rs            # SSE 端点：flow_stream()（订阅 FlowAggregator broadcast channel）
└── routes/           # HTTP 路由
    ├── mod.rs        # api_router() 合并各 Service 子路由
    ├── ping.rs       # /api/ping 路由
    ├── collection.rs # /api/workspaces, /api/folders, /api/requests, /api/environments 路由
    ├── request_service.rs # /api/requests/send 路由
    ├── proxy.rs      # /api/proxy/start, /api/proxy/stop 路由
    ├── flow.rs       # /api/flows 路由
    └── system.rs     # /api/system 路由（证书导出等）

build.rs              # 代码生成：扫描 #[rpc_method] 清单 → 生成 TS 客户端 + CLI Rust 客户端
```

## 设计原则

- **只做业务 API**，不承担任何静态文件/前端页面的托管职责。
- 桌面 UI 的页面统一由 Tauri 经 `tauri://` 协议加载，与本 Server 完全无关。

## 依赖关系

- 被 `cuckoo-desktop`（spawn_server 内嵌拉起）依赖
- 依赖 `cuckoo-service`（业务逻辑）、`cuckoo-core`（错误类型 + RPC 登记）、`cuckoo-store`（数据库连接）
- 依赖 `axum`、`tokio`、`tower-http`、`tracing`
