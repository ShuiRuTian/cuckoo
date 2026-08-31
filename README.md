# Cuckoo

> 一个集 API 客户端 + MITM 抓包代理于一体的本地调试工具，对标 Postman + Charles/Reqable。

基于 Tauri 2.x + React + TypeScript 前端，Rust workspace 后端，支持桌面 GUI、CLI、MCP 三种使用方式。

---

## 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                        apps/（应用入口）                         │
│                                                                  │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐        │
│  │ cuckoo-desktop│   │ cuckoo-server│   │ cuckoo-cli   │        │
│  │  (Tauri 壳)   │──▶│  (axum HTTP  │◀──│  (clap CLI)  │        │
│  │  tauri:// UI  │   │   +SSE API)  │   │              │        │
│  └──────────────┘   └──────┬───────┘   └──────────────┘        │
│                            │                                     │
│                     ┌──────▼───────┐   ┌──────────────┐        │
│                     │ cuckoo-mcp   │   │              │        │
│                     │ (MCP Server) │   │              │        │
│                     └──────────────┘   └──────────────┘        │
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                    crates/（核心库）                              │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ cuckoo-service ★ (业务逻辑层，唯一包含 #[rpc_method] 方法) │   │
│  │  依赖: cuckoo-core, cuckoo-store, cuckoo-dto,            │   │
│  │        cuckoo-http, cuckoo-templates                     │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │cuckoo-core│  │cuckoo-   │  │cuckoo-   │  │cuckoo-   │        │
│  │(错误+RPC │  │macros    │  │store     │  │dto       │        │
│  │ 登记表)  │  │(属性宏)  │  │(SQLite)  │  │(API契约) │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │cuckoo-   │  │cuckoo-   │  │cuckoo-   │  │cuckoo-   │        │
│  │http      │  │templates │  │ws (M4)   │  │graphql   │        │
│  │(reqwest) │  │(变量插值)│  │          │  │(M4)      │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │cuckoo-   │  │cuckoo-ca │  │cuckoo-   │  │cuckoo-   │        │
│  │proxy(M2) │  │(M2 证书)│  │flow(M2)  │  │platform  │        │
│  │(MITM内核)│  │          │  │(Flow模型)│  │(M2 系统) │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Crate 一览

### 核心库（crates/）

| Crate | 功能 | 状态 | README |
|---|---|---|---|
| **cuckoo-core** | 公共错误类型 + RPC 方法登记表 | ✅ M0 完成 | [→](crates/cuckoo-core/README.md) |
| **cuckoo-macros** | `#[rpc_method]` 属性宏（编译期路由元信息收集 + 代码生成） | ✅ M0 完成 | [→](crates/cuckoo-macros/README.md) |
| **cuckoo-store** | SQLite 存储层（SeaORM Entity + 迁移 + Repository） | ✅ M1 完成 | [→](crates/cuckoo-store/README.md) |
| **cuckoo-dto** | API 契约层（DTO 类型 + ts-rs 导出 + Entity 转换） | ✅ M1 完成 | [→](crates/cuckoo-dto/README.md) |
| **cuckoo-http** | HTTP 客户端引擎（reqwest 封装 + 计时） | ✅ M1 完成 | [→](crates/cuckoo-http/README.md) |
| **cuckoo-templates** | 变量插值引擎（`{{var}}` 渲染） | ✅ M1 完成 | [→](crates/cuckoo-templates/README.md) |
| **cuckoo-service** ★ | Service 层（唯一业务逻辑，`#[rpc_method]` 标注） | ✅ M1 完成 | [→](crates/cuckoo-service/README.md) |
| **cuckoo-proxy** | MITM 代理内核（完全自研） | ✅ M2 完成 | [→](crates/cuckoo-proxy/README.md) |
| **cuckoo-ca** | 证书体系（根 CA 生成 + 叶子证书签发） | ✅ M2 完成 | [→](crates/cuckoo-ca/README.md) |
| **cuckoo-flow** | Flow 数据模型 + 批量聚合器 | ✅ M2 完成 | [→](crates/cuckoo-flow/README.md) |
| **cuckoo-platform** | 系统集成（代理设置 + CA 安装） | ✅ M2 完成 | [→](crates/cuckoo-platform/README.md) |
| **cuckoo-ws** | WebSocket 客户端 | 🔲 M4 待实现 | [→](crates/cuckoo-ws/README.md) |
| **cuckoo-graphql** | GraphQL 请求辅助 | 🔲 M4 待实现 | [→](crates/cuckoo-graphql/README.md) |

### 应用入口（apps/）

| App | 功能 | 状态 | README |
|---|---|---|---|
| **cuckoo-server** | 本地 HTTP+SSE Server（axum），唯一业务协议入口 | ✅ M0/M2 完成 | [→](apps/cuckoo-server/README.md) |
| **cuckoo-desktop** | Tauri 应用外壳，spawn server + tauri:// UI | ✅ M0 完成 | [→](apps/cuckoo-desktop/src-tauri/README.md) |
| **cuckoo-cli** | 命令行工具，Server 的 HTTP/SSE 客户端 | ✅ 骨架完成，可运行 | [→](apps/cuckoo-cli/README.md) |
| **cuckoo-mcp** | MCP Server，AI Agent 工具接口 | 🔲 M5 待实现 | [→](apps/cuckoo-mcp/README.md) |

---

## 顶层目录结构

```
cuckoo/
├── Cargo.toml              # workspace 根配置（members + workspace.dependencies）
├── plan.md                 # 分阶段实施路线图（M0-M5 + v2+）
├── spec.md                 # 技术规格说明书
├── investigation.md        # 技术调研文档
├── README.md               # ← 本文件
│
├── crates/                 # 核心库（Tauri 无关）
│   ├── cuckoo-core/        #   错误类型 + RPC 登记
│   ├── cuckoo-macros/      #   #[rpc_method] 属性宏
│   ├── cuckoo-store/       #   SQLite 存储层
│   ├── cuckoo-dto/         #   API 契约（DTO + ts-rs）
│   ├── cuckoo-http/        #   HTTP 客户端引擎
│   ├── cuckoo-templates/   #   变量插值引擎
│   ├── cuckoo-service/     #   ★ Service 层（业务逻辑）
│   ├── cuckoo-proxy/       #   MITM 代理内核（M2）
│   ├── cuckoo-ca/          #   证书体系（M2）
│   ├── cuckoo-flow/        #   Flow 数据模型（M2）
│   ├── cuckoo-platform/    #   系统集成（M2）
│   ├── cuckoo-ws/          #   WebSocket 客户端（M4）
│   └── cuckoo-graphql/     #   GraphQL 辅助（M4）
│
├── apps/                   # 应用入口
│   ├── cuckoo-server/      #   HTTP+SSE Server（axum）
│   ├── cuckoo-desktop/     #   Tauri 桌面应用
│   ├── cuckoo-cli/         #   命令行工具
│   └── cuckoo-mcp/         #   MCP Server（M5）
│
├── src/                    # 前端代码（React + TypeScript + Vite）
│   ├── App.tsx             #   顶层布局（Client/Proxy Tab）
│   ├── main.tsx            #   React 入口
│   ├── components/         #   通用组件
│   │   ├── custom/         #     自研组件（ResizablePanel / KeyValueEditor）
│   │   └── ui/             #     shadcn/ui 组件
│   ├── features/           #   功能模块
│   │   ├── collections/    #     Collection 树 / Workspace 选择器
│   │   ├── request-builder/#     请求编辑器 / 响应查看器
│   │   └── settings/       #     环境变量管理
│   ├── lib/                #   工具库
│   │   ├── api/            #     API 客户端
│   │   │   ├── client.ts   #       apiFetch 封装（自动携带 token）
│   │   │   ├── token.ts    #       get_server_token() 调用
│   │   │   └── generated/  #       ⚙️ 自动生成（api.ts / types.ts / index.ts）
│   │   └── utils.ts        #     通用工具函数
│   └── state/              #   Jotai 状态管理
│       └── app.ts          #     全局状态 atoms
│
└── package.json            # 前端依赖 + scripts（pnpm tauri dev）
```

---

## 关键设计决策

### 1. 写一次 Service 方法，自动出现 REST 端点

```
#[rpc_method("GET", "/api/ping")]
pub async fn ping() -> ServiceResult<PongResponse> { ... }
```

→ 编译期自动：
- 生成 axum 路由
- 写入 `.rpc_routes.json` 清单
- `build.rs` 读取清单 → 生成前端 TS `fetch` 封装 + CLI Rust 客户端

### 2. 页面加载与业务 API 分离

- 页面加载 → Tauri `tauri://` 协议（性能 + 安全）
- 业务请求 → `cuckoo-server` HTTP 端口（标准 fetch / EventSource）

### 3. 四个平级入口共享同一 Service 层

```
cuckoo-desktop (GUI)  ──┐
cuckoo-cli (CLI)      ──┼──▶ cuckoo-server ──▶ cuckoo-service
cuckoo-mcp (AI Agent) ──┘
```

---

## 快速开始

```bash
# 安装依赖
pnpm install

# 开发模式（同时启动 Tauri + Vite + cuckoo-server）
pnpm tauri dev

# 构建
pnpm tauri build

# 仅编译 Rust workspace
cargo build --workspace
```

---

## 实施进度

详见 `plan.md`。当前进度：**M2 核心已实现**（代理内核、证书体系、Flow 管道、系统代理集成、SSE 端点），下一步推进 **M1 剩余前端任务**（Collection 树 UI、请求编辑器、响应查看器）及 **M2 前端 UI**（流量列表、代理控制面板）。
