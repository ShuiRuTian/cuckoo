# cuckoo-cli

> 命令行工具，作为 cuckoo-server 的 HTTP/SSE 客户端。

## 功能

- **自动生成 RPC 客户端**：通过 `#[rpc_method]` 宏生成的 `cli_generated.rs` 调用 Server 的 REST 端点，无需手动维护路由。
- **Headless Server 自动拉起**：如果本地没有运行中的 Server，CLI 会自动拉起一个 headless `cuckoo-server` 子进程。
- **Server 探测**：通过读取 `server.port` 和 `server.token` 文件发现并连接本地运行的 Server。
- **子命令**：`send` / `proxy` / `collection` / `flow` / `server` / `version`

## 子命令

| 子命令 | 功能 | 状态 |
|---|---|---|
| `send` | 发送一次性 HTTP 请求 | ✅ 已完成 |
| `collection` | Workspace / Folder / Request / Environment CRUD | ✅ 已完成 |
| `proxy` | 代理生命周期管理（start / stop / status） | ✅ 骨架完成 |
| `flow` | 抓包流量查询（list --follow / show） | ✅ 骨架完成 |
| `server` | 显式拉起本地 cuckoo-server（start / stop） | ✅ 已完成 |
| `version` | 打印版本信息 | ✅ 已完成 |

## 目录结构

```
src/
├── main.rs           # CLI 入口：clap 解析子命令 + tokio runtime
├── server.rs         # ensure_server() / detect_server() / connect_or_none()：Server 探测与自动拉起
├── commands/         # 子命令实现
│   ├── mod.rs
│   ├── send.rs       # send：发送一次性 HTTP 请求
│   ├── proxy.rs      # proxy：代理生命周期管理
│   ├── collection.rs # collection：Collection CRUD
│   ├── flow.rs       # flow：抓包流量查询
│   └── server.rs     # server：显式拉起本地 cuckoo-server
└── generated/        # 自动生成的代码（.gitignore 排除）
    ├── mod.rs
    └── cli_generated.rs  # 由 cuckoo-server/build.rs 自动生成
```

## 设计原则

- CLI 不依赖 `cuckoo-service` / `cuckoo-dto`，只关心 JSON 的序列化/反序列化。
- 生成的代码统一用 `serde_json::Value` 做 body 和返回值。

## 依赖关系

- 依赖 `cuckoo-server`（build.rs 生成客户端代码，运行时探测/拉起 Server）
- 依赖 `clap`、`reqwest`、`tokio`、`anyhow`、`serde_json`
