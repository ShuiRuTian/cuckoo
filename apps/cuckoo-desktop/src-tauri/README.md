# cuckoo-desktop

> Tauri 应用外壳 —— 纯粹的"壳"，不含任何业务 command。

## 功能

- **启动时拉起 cuckoo-server**：`tokio::spawn` 在同进程内启动 `cuckoo-server`，监听 `127.0.0.1` 的空闲端口。
- **Tauri State 共享**：将 server 的地址和 token 存入 `ServerState`，供前端通过 `get_server_token()` Tauri command 拉取。
- **前端页面加载**：始终沿用 Tauri 原生的 `tauri://` 自定义协议加载打包进二进制的前端静态资源（不走 `http://127.0.0.1`）。
- **system_commands**：`get_server_token()` Tauri command 供前端启动时获取鉴权 token。

## 目录结构

```
src/
├── lib.rs              # run() 入口：spawn server → manage ServerState → invoke_handler
├── main.rs             # main() → lib::run()
├── state.rs            # ServerState 结构体（addr + token）
└── system_commands.rs  # get_server_token() Tauri command
```

## 设计原则

- 桌面 UI 的页面加载和业务 API 请求走两条完全分离的链路：
  - 页面加载 → Tauri `tauri://` 协议
  - 业务请求 → `cuckoo-server` HTTP 端口

## 依赖关系

- 依赖 `cuckoo-server`（spawn_server）、`tauri`
- 前端代码在项目根目录 `src/` 下（React + TypeScript + Vite）
