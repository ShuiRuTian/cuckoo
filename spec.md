# 产品与技术规格：Cuckoo

> 本文档定义产品范围、架构设计、数据模型、模块划分与技术选型。基于 `investigation.md` 中的调研结论。

---

## 1. 产品定位

**Cuckoo** 是一个桌面应用（Tauri），合并两类工具的核心能力：

1. **API 客户端**：像 Postman/Hoppscotch/Bruno/Yaak 一样，手工构造并发送 HTTP、GraphQL、WebSocket 请求，管理 Collection、环境变量、认证信息。
2. **MITM 抓包调试代理**：像 Charles/Fiddler/HTTP Toolkit/mitmproxy 一样，作为中间人代理，拦截、展示、修改（拦截规则/断点/Map Local/Map Remote/重写规则）经过它的 HTTP/1.1、HTTP/2、WebSocket 流量（HTTP/3 MITM 明确排除在 v1 范围外，见第 7 节）。

两个模块共享同一个 Rust 网络核心（HTTP 客户端引擎、TLS、证书体系、Flow 数据模型、存储层），在同一个应用里通过"请求发送"和"流量捕获"两个视角联动使用（例如：从抓包记录一键转成可编辑请求放入 Collection 重新发送——这是 Reqable 的招牌功能之一）。

### 1.1 目标用户与核心场景

- 移动/Web 后端联调：拦截 App 真实请求，查看/修改 header、body，重放。
- API 设计与测试：手工编排请求集合，配环境变量做多环境切换（dev/staging/prod）。
- 调试第三方 SDK/GraphQL 后端：抓包看到底发了什么、返回了什么。
- WebSocket 长连接调试：查看逐帧消息，模拟服务端下发消息。

### 1.2 非目标（明确不做，避免范围蔓延）

- HTTP/3 (QUIC) 的 MITM 拦截（技术风险评估见 `investigation.md` 3.6 节）—— v1 不做，仅提供 HTTP/3 客户端发送能力（Beta）。
- 移动端免配置抓包（WireGuard 模式/eBPF 透明代理）—— 远期方向，v1 用标准系统代理配置 + 手动安装证书。
- 团队协作/云同步（Postman Cloud、Hoppscotch Teams 那类后端服务）—— 本地优先（local-first），暂不做云端账号体系。
- Frida 动态注入反 SSL Pinning —— 列为可能的插件/扩展方向，不在核心范围。

---

## 2. 总体架构

### 2.1 分层原则：Tauri-free 核心 + Service 层 + 单一 HTTP+SSE Server

Rust 侧的分层比"核心逻辑不依赖 `tauri::AppHandle`"这个最低要求更进一步：**所有业务逻辑收敛到一个不依赖任何具体传输协议的 Service 层**，对外只有唯一一条协议路径——`cuckoo-server` 提供的 **HTTP（请求-响应）+ SSE（服务端推送）**。桌面 UI、CLI、MCP Server 三类客户端全部是这个 Server 的网络客户端，共用同一套路由、同一套事件模型、同一套鉴权，不存在任何客户端专属的协议分支。**前端页面本身只有一种加载方式**——由 `cuckoo-desktop` 打包进二进制、经 Tauri 官方 `tauri://` 协议加载，`cuckoo-server` 不承担、也不需要承担任何静态文件托管职责。这个设计的动机与业界调研依据见 2.2 节（并见 `investigation.md` 3.12 节）。

```
cuckoo/
├── investigation.md            # 调研文档
├── spec.md                     # 本文档
├── plan.md                     # 实施计划
├── crates/                     # Tauri 无关的核心 crate（workspace 顶层，非 src-tauri 内部）
│   ├── cuckoo-core/             # 公共类型、错误处理、异步工具
│   ├── cuckoo-store/            # SQLite 存储层（sea-orm + sea-orm-migration，sqlx-sqlite 驱动）
│   ├── cuckoo-http/             # HTTP 客户端引擎（reqwest 封装 + 精确计时）
│   ├── cuckoo-ws/                # WebSocket 客户端（tokio-tungstenite 封装）
│   ├── cuckoo-graphql/          # GraphQL 请求构造/内省辅助（薄层，复用 cuckoo-http）
│   ├── cuckoo-proxy/             # MITM 代理内核（完全自研，不依赖第三方代理封装库，详见第 4 节）
│   ├── cuckoo-ca/                # 证书体系：根 CA 生成/持久化/安装引导，叶子证书签发
│   ├── cuckoo-flow/              # Flow/Transaction 数据模型 + 序列化 + 环形缓冲存储
│   ├── cuckoo-platform/          # 系统集成：代理设置、CA 信任安装（分平台实现）
│   ├── cuckoo-templates/        # 变量插值引擎（{{var}} 渲染，环境变量解析链）
│   └── cuckoo-service/          # ★ Service 层：唯一包含业务逻辑的地方，见 2.2 节
│       ├── request_service.rs   # send_request()/replay_flow() 等
│       ├── collection_service.rs # Workspace/Folder/Request/Environment CRUD
│       ├── proxy_service.rs      # start_proxy()/stop_proxy()/subscribe_flows() -> Stream<FlowEvent>
│       ├── rule_service.rs       # 拦截规则 CRUD、resume_intercept()
│       └── system_service.rs     # 证书导出、系统代理设置
├── apps/                        # 四个入口，均只依赖 cuckoo-service（直接或经由 cuckoo-server），不含业务逻辑
│   ├── cuckoo-desktop/           # Tauri 应用外壳——纯粹的"壳"，不含任何业务 command
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   └── src/
│   │       ├── main.rs           # 启动时 tokio::spawn 拉起本地 cuckoo-server（同进程）供业务 API 使用；WebviewWindow 仍然通过 Tauri 原生的 `tauri://` 自定义协议加载打包好的前端静态资源（不走 http://127.0.0.1 去拿页面，理由见 2.2 节）
│   │       ├── system_commands.rs # 极少数必须原生实现的能力：托盘菜单、原生文件对话框、开机自启注册，以及 `get_server_token()`——唯一一个业务相关的 Tauri command，供前端启动时拉取鉴权 token（见 6.3 节）
│   │       └── state.rs          # 持有 cuckoo-server 的 JoinHandle 与端口/token 信息
│   ├── cuckoo-server/            # 本地 HTTP+SSE Server（axum）——唯一的业务协议入口，可内嵌启动也可独立常驻进程
│   │   └── src/
│   │       ├── main.rs           # 独立运行入口（`cuckoo-server --headless --port 4173`）
│   │       ├── routes/           # REST 路由（由 #[rpc_method] 宏收集的方法清单自动生成 Router，见 2.4 节）
│   │       ├── sse.rs             # SSE 事件推送端点（/api/flows/stream 等）
│   │       └── auth.rs            # Authorization: Bearer <token> 鉴权中间件 + Origin/CORS 校验（放行 tauri:// 页面源）
│   ├── cuckoo-cli/               # 命令行工具，作为 cuckoo-server 的 HTTP/SSE 客户端
│   │   └── src/
│   │       ├── main.rs
│   │       └── commands/         # `cuckoo send` / `cuckoo proxy start` / `cuckoo flow list` 等子命令
│   └── cuckoo-mcp/               # MCP Server，同样作为 cuckoo-server 的 HTTP 客户端（或进程内直连 service）
│       └── src/
│           ├── main.rs
│           └── tools/            # 按 MCP tool 分文件：send_request/list_flows/create_rule/resume_intercept 等
└── src/                          # 前端（React + TS + Vite），唯一的加载方式是被 cuckoo-desktop 打包进二进制、由 Tauri 经 tauri:// 协议加载；cuckoo-server 不提供任何静态文件托管能力
    ├── features/
    │   ├── request-builder/      # API 客户端请求编辑器
    │   ├── collections/          # 侧边栏 Collection 树
    │   ├── proxy-capture/        # 抓包流量列表 + 详情面板
    │   ├── intercept-rules/      # 拦截规则/断点/Map Local 配置 UI
    │   └── settings/             # 证书安装向导、代理设置、通用设置
    ├── components/
    │   ├── ui/                   # shadcn/ui 生成的基础组件（基于 Base UI）
    │   └── custom/               # 业务层自定义组件（KeyValueEditor、FlowTable、CollectionTree、ResizablePanel 等）
    ├── lib/
    │   └── api/                   # fetch 封装的强类型 API 客户端（生成）+ EventSource 事件订阅封装 + 生成的 TS 类型（ts-rs 产出）
    └── state/                     # Jotai atoms
```

`apps/` 目录下四个 crate 彼此平级，`cuckoo-desktop`（桌面）不比 `cuckoo-cli`/`cuckoo-mcp` 更"核心"——这是有意的设计：任何一个入口都不能变成另一个入口的依赖。`cuckoo-desktop` 自身甚至不直接依赖 `cuckoo-service`，只依赖 `cuckoo-server`（把它当一个库 crate 内嵌启动）；`cuckoo-cli`/`cuckoo-mcp` 则通过网络请求间接依赖 `cuckoo-server` 暴露的协议，避免出现"CLI 依赖桌面壳里的某个工具函数"这种耦合。

### 2.2 后端架构核心决策：为什么需要一个本地 Server，以及为什么桌面 UI 也走它

**这个决策相对最初的设想（"不需要独立 Server"）有调整**：最初的结论成立于"只有一个 Tauri 桌面 App、UI 和后端天然在同一进程"这个前提，但用户明确要求 **CLI 和 MCP Server 都要具备与桌面 UI 对等的操作能力**（发请求、管理 Collection、启停代理、查询/订阅实时抓包流量、管理拦截规则、处理断点），这个新要求打破了原有前提。

**为什么不能只靠"CLI/MCP 直接读写同一个 SQLite 文件"绕过（Yaak CLI 的模式）**：Yaak 的 CLI 之所以能这样做，是因为它面对的都是**静态数据 CRUD**（Collection/Request 是存量数据）。我们的 MITM 代理是一个**有状态的长驻进程**——是否在监听、断点当前卡住了哪些请求、实时产生的 Flow 事件流，这些都是运行时内存状态，不落盘在 SQLite 里，CLI/MCP 只读数据库看不到这些信息，也无法对断点下达放行/丢弃的决策。

**为什么桌面 UI 的业务 API 调用也应该走同一个 Server，而不是保留 Tauri IPC 作为单独的高性能路径**：这是本轮设计与最初方案最大的差异。最初考虑过"桌面 UI 走 Tauri command（高性能），CLI/MCP 走本地 Server（兼容性）"的双轨方案，但进一步调研发现这会引入真实的工程代价：Service 层每新增一个方法/事件，都要同时写 Tauri command 包装 + REST 端点包装，且两边的事件类型需要人工保持同步——这正是应该被消灭的"胶水代码"。而本地 loopback HTTP 的延迟（个位数到低两位数毫秒）对人机交互而言完全淹没在渲染开销中，没有充分理由为性能而保留双轨。因此确定：**桌面 UI 的业务 API 调用也作为 `cuckoo-server` 的一个普通网络客户端**，Tauri 在业务逻辑层面彻底退化为"壳"。需要特别注意的是：这里的"壳"仅指业务逻辑层面——Tauri 在"页面资源如何加载进窗口"这件事上仍然扮演正常角色（见下方第 3 点），不存在"连窗口资源加载也改走 HTTP"这一步。详细的技术论证、与 OpenCode/Claude Code/MCP 官方实践的对齐见 `investigation.md` 3.12 节，这里直接给落地设计：

1. **Service 层**（`cuckoo-service`）是唯一包含业务逻辑的地方，函数签名不出现任何 Tauri 或其他传输层专属类型，高频事件（Flow 流、断点通知）通过 `tokio::sync::broadcast` 对外暴露订阅接口。
2. **唯一协议入口**：`cuckoo-server`（`axum`）直接调用 `cuckoo-service`，对外提供 REST 路由做请求-响应式操作、SSE 端点做事件订阅。无论请求来自桌面 WebView、CLI 还是 MCP，走的都是同一条代码路径，不存在分支；`cuckoo-server` 只做业务 API，不承担任何静态文件/前端页面的托管职责。
3. **桌面 UI 路径（页面加载与业务 API 是两条通道）**：`cuckoo-desktop` 启动时在同进程内用 `tokio::spawn` 拉起 `cuckoo-server`（监听 `127.0.0.1` 固定或可配置端口）专门服务业务 API；Tauri 创建的 `WebviewWindow` **始终通过 Tauri 官方的 `tauri://` 自定义协议加载打包进二进制的前端静态资源**，而不是让窗口本身也去请求 `http://127.0.0.1:<port>/`——这样才能保留 Tauri 官方资源加载机制的性能优势（不经过网络栈）和安全特性（专属 CSP/隔离上下文），也不需要 `cuckoo-server` 背负任何静态文件服务器职责。页面加载完成后，前端 JS 代码用标准 `fetch`/`EventSource` 访问同一个 `cuckoo-server` 端口处理业务数据——**页面本身怎么被加载进来（`tauri://`）和页面加载完成后怎么访问业务数据（`http://127.0.0.1:<port>`）是两个独立的问题，前者恒定不变，不存在开关或可选项**。`cuckoo-server` 也可以脱离桌面壳独立运行（`cuckoo-server --headless`），供"只想用命令行/AI 操作、不需要 GUI"的场景使用，此时它是一个独立进程持有自己的 `cuckoo-service` 实例。
4. **CLI/MCP 路径**：`cuckoo-cli`、`cuckoo-mcp` 都不直接链接 `cuckoo-service` 或触碰 SQLite，而是作为 `cuckoo-server` 的 HTTP/SSE 客户端。如果检测到本地没有运行中的 Server（既没有桌面 App 也没有独立 headless Server），CLI 可以选择临时拉起一个 headless Server 子进程执行完命令后退出，适合一次性/CI 场景（类比 `ollama` CLI 自动拉起本地服务的模式）；也可以要求用户显式先启动 Server 再连接，具体交互方式留到实现阶段按体验打磨。
5. **本地 Server 鉴权**：即使只监听 loopback 地址，应用数据目录下生成一个 token 文件（如 `~/Library/Application Support/Cuckoo/server.token`），所有 REST 请求和 SSE 订阅都必须携带标准 `Authorization: Bearer <token>` 请求头，`cuckoo-server` 中间件校验，`cuckoo-cli`/`cuckoo-mcp` 启动时读取该文件；桌面 UI 场景下，由于页面是经 `tauri://` 加载的（不是普通网页，拿得到 Tauri 的 IPC 能力），用一个极薄的 `get_server_token()` Tauri command 让前端在启动时主动向 Tauri 主进程要 token，比"URL 参数/页面注入全局变量"这类专为普通网页设计的变通方案更干净（token 不会残留在 URL 或页面 HTML 里）——防止同机其他未授权进程调用，也避免了 WebSocket 场景下"握手阶段无法带标准 Authorization 头"的变通方案（详见 `investigation.md` 3.12/3.14 节）。

**这个架构对旧有结论的取舍**：HTTP Toolkit/Hoppscotch 需要独立 server 是因为它们的 UI 可以脱离桌面壳跑纯 Web 版本，这个理由对我们不成立（我们明确不做纯 Web 版，也不提供任何方式的前端页面 HTTP 访问）；我们需要独立 Server 的唯一理由是**服务"CLI 和 MCP 这两个天然运行在 Tauri 进程之外的客户端"，并让桌面 UI 自身享受到"一份协议、零胶水"的好处**。

### 2.3 少写胶水代码：从 Service 方法自动生成路由与客户端

新增一个能力时，应该只需要在 `cuckoo-service` 里写一次函数，其余层面（REST 路由、SSE 事件类型、TS 客户端、CLI 子命令、MCP tool）尽可能自动派生，而非手写多份并人工保持一致（完整论证见 `investigation.md` 3.13 节）：

1. Service 方法用 `#[rpc_method("POST", "/api/flows/:id/resume")]` 属性宏标注，宏在编译期把方法注册进一张路由表，`cuckoo-server` 启动时根据这张表自动拼装 `axum::Router`，新增方法不需要手写单独的 handler 注册代码。
2. 同一个属性宏也把入参/返回类型（已用 `#[derive(Serialize, Deserialize, TS)]` 标注）收集进一份编译期可枚举的"方法清单"，一个构建脚本（`build.rs` 或独立 codegen 二进制）遍历该清单，为前端生成强类型的 `fetch` 封装函数（`lib/api/generated.ts`），为 CLI 生成通用调用入口，为 MCP 生成 tool schema（基于 `schemars`）。
3. 这套机制的具体实现难度与项目体量匹配：v1 方法数量不多时可以先手写一份简单的清单式 `build.rs`，不必一开始就追求完全自动反射。详细设计见 7.2～7.4 节。

### 2.4 高层数据流图

```
【页面资源加载】                          【业务 API 调用】
桌面 UI 窗口 --tauri://--> 打包的前端静态资源     桌面 UI / cuckoo-cli / cuckoo-mcp
（不经过网络，仅本地资源读取，唯一加载方式）        均通过 fetch / EventSource 访问 cuckoo-server

┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  桌面 UI      │  │  cuckoo-cli   │  │  cuckoo-mcp   │  ← AI Agent 通过 MCP 协议调用
│(Tauri WebView，│  │ （命令行工具）  │  │ （MCP Server） │
│ 页面经tauri://加载)│              │              │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │ fetch/EventSource │ HTTP/SSE        │ HTTP（或进程内直连）
       │ (本地 loopback)   │ (本地 loopback) │ (本地 loopback)
       ▼                  ▼                 ▼
┌──────────────────────────────────────────────────────────────────┐
│           cuckoo-server (axum)，唯一协议入口，可内嵌于 cuckoo-desktop 或独立常驻 │
│           （只提供业务 API，不托管任何静态文件/前端页面）              │
│  routes/*.rs (REST) ──┐          ┌── sse.rs (事件推送)   │
│                      ▼          ▼                              │
│              ┌─────────────────────────────┐                │
│              │        cuckoo-service         │  ← 唯一业务逻辑层│
│              └───────────────┬───────────────┘                │
│      ┌───────────────────────┼────────────────────────┐      │
│      ▼                       ▼                        ▼      │
│  cuckoo-http (reqwest)  cuckoo-proxy (自研引擎)   cuckoo-store  │
│      │ 发起真实请求          │ 监听本地端口            │ (SQLite) │
│      │                     │ TLS 终止/证书签发        │ 持久化   │
│      │                     │ 转发到真实服务器          │         │
│      ▼                     ▼                                  │
│           统一 Flow 事件（cuckoo-flow）──批量聚合──┐            │
└──────────────────────────────────────────────────┼────────────┘
                 │                                  │ 分发给上方三类 SSE 订阅者
                 ▼                                  ▼
         真实目标服务器                     被拦截的客户端进程
                                       (浏览器/手机 App via 系统代理/curl)
```

**页面资源加载与业务 API 调用是两条独立的通道**：桌面 UI 的 `WebviewWindow` 通过 Tauri 官方 `tauri://` 自定义协议加载打包进二进制的前端静态资源，这一步完全不经过 `cuckoo-server`、不经过网络栈，也是前端页面**唯一**的加载方式；页面加载完成后，前端 JS 发起的业务请求（`fetch`/`EventSource`）才会经由 `http://127.0.0.1:<port>` 访问 `cuckoo-server`。`cuckoo-server` 全程只扮演业务 API 的角色，不存在、也不需要存在托管前端静态资源的能力或开关。

当 `cuckoo-server` 由 `cuckoo-desktop` 内嵌拉起时，二者处于同一进程（`tokio::spawn` 共享 runtime），但 Tauri 主进程与 `cuckoo-server` 之间不存在任何业务相关的进程内直连调用——WebView 前端的业务 API 调用仍然必须经由网络请求（`http://127.0.0.1:<port>`）访问 `cuckoo-server`，与完全独立进程运行时行为一致；但 WebView 加载页面本身这一步不受此约束，走的是 Tauri 原生资源协议。`cuckoo-server` 也可以不随 `cuckoo-desktop` 启动，而是独立编译运行（`cuckoo-server --headless`），此时它自己持有一份 `cuckoo-service` 实例，图中框内的内容原样搬到独立进程里，其余两类客户端（CLI、MCP）的调用方式不变。

---

## 3. 核心数据模型

### 3.1 设计原则

- Rust struct 是唯一真源（single source of truth），通过 `ts-rs` 自动生成前端 TypeScript 类型，避免手写两份 schema 漂移（参考 investigation.md 中 Bruno 踩过的坑）。
- Flow（抓包记录）与 Request（API 客户端里编排的请求）是两个独立但可互相转换的模型：抓包记录是"被动观察到的历史事实"，Request 是"主动可编辑可重发的模板"。二者共享 `HttpMessage`（headers/body/method/url 等）底层结构。
- 所有面向"大数据量、高频写入"的模型（Flow、WebSocket 帧）都要考虑惰性加载 body、环形缓冲上限。

### 3.2 Collection / Request（API 客户端侧，参考 Yaak 的 SQLite 关系模型）

以下是核心业务 struct 的简化定义（字段设计），实际持久化实现上，`cuckoo-store` 用 **`sea-orm`** 承载：每张表对应一个 `sea-orm` Entity 模块（`DeriveEntityModel` 宏生成 `Model`/`ActiveModel`/`Column`/`Relation`），下面的 struct 字段基本对应 Entity 的 `Model` 字段（`Vec<HeaderEntry>`/`WorkspaceSettings` 这类复合结构以 JSON 列存储，用 `sea-orm` 的 `with-json` feature + 手写 `TryGetable`/`Into<Value>` 或直接存 `Json` 列类型）。`Workspace`→`Folder`→`HttpRequestDef`、`Workspace`→`Environment` 这几组一对多关系用 `Related`/`RelationTrait` 声明，查询整棵 Collection 树时用 `find_with_related()` 一次性加载，避免 N+1 查询。表结构的创建与演进通过 `sea-orm-migration` 的版本化迁移文件管理，不再手写裸 SQL 迁移脚本。

```rust
// cuckoo-store 中的核心业务字段（简化，实际以 sea-orm Entity 承载）
struct Workspace {
    id: String,
    name: String,
    base_headers: Vec<HeaderEntry>,
    settings: WorkspaceSettings,   // TLS校验开关/代理设置/超时等
    created_at: i64,
    updated_at: i64,
}

struct Folder {
    id: String,
    workspace_id: String,
    parent_folder_id: Option<String>,
    name: String,
    sort_key: f64,               // 支持拖拽排序
}

struct HttpRequestDef {           // Collection 里保存的"请求模板"
    id: String,
    folder_id: Option<String>,
    workspace_id: String,
    name: String,
    method: String,
    url: String,                 // 含 {{variable}} 模板语法
    headers: Vec<HeaderEntry>,
    query_params: Vec<KeyValueEntry>,
    body: RequestBody,           // enum: None/Raw{content_type,text}/FormData/UrlEncoded/Binary/GraphQL{query,variables}
    auth: AuthConfig,            // enum: None/Basic/Bearer/ApiKey/OAuth2/AWSSigV4/Digest
    pre_request_script: Option<String>,
    post_response_script: Option<String>,
    sort_key: f64,
}

struct Environment {
    id: String,
    workspace_id: String,
    name: String,
    variables: Vec<EnvVariable>,  // {key, value, secret: bool, enabled: bool}
}

struct HeaderEntry { name: String, value: String, enabled: bool }
struct KeyValueEntry { key: String, value: String, enabled: bool }
```

### 3.3 Flow / Transaction（抓包侧，参考 CDP Network domain + mitmproxy Flow）

```rust
// cuckoo-flow
#[derive(Serialize, Deserialize, Clone, TS)]
#[ts(export)]
pub struct Flow {
    pub id: String,                      // ULID，保证时间有序
    pub protocol: FlowProtocol,          // Http1 | Http2 | WebSocket | Http3(reserved)
    pub client_addr: SocketAddrInfo,
    pub server_addr: Option<SocketAddrInfo>,
    pub request: HttpMessage,
    pub response: Option<HttpMessage>,
    pub timing: FlowTiming,
    pub tls: Option<TlsInfo>,
    pub websocket_frames: Vec<WsFrame>,   // 仅 protocol=WebSocket 时非空
    pub status: FlowStatus,               // Pending | Complete | Error | Intercepted
    pub error: Option<String>,
    pub intercept: InterceptState,        // NotIntercepted | Paused{stage} | Resumed
    pub tags: Vec<String>,                // "modified" / "breakpoint-hit" 等
}

pub struct HttpMessage {
    pub start_line: String,               // 如 "GET /api/users HTTP/1.1"
    pub headers: Vec<HeaderEntry>,        // 保序，允许重复 key
    pub headers_raw: Option<String>,      // 原始字节块，用于精确还原
    pub body_ref: Option<BodyRef>,        // 惰性引用，不内联大 body
    pub body_size: usize,
    pub body_truncated: bool,
}

pub struct BodyRef {
    pub storage_key: String,              // 指向 blob 存储（独立 SQLite 或临时文件）
    pub content_type: Option<String>,
    pub encoding: Option<String>,         // gzip/br/deflate，供前端决定是否需要解压展示
}

pub struct FlowTiming {
    pub start_time: i64,                  // epoch ms
    pub dns_start: Option<i64>,  pub dns_end: Option<i64>,
    pub connect_start: Option<i64>, pub connect_end: Option<i64>,
    pub tls_start: Option<i64>,  pub tls_end: Option<i64>,
    pub send_start: Option<i64>, pub send_end: Option<i64>,
    pub ttfb: Option<i64>,
    pub end_time: Option<i64>,
}

pub struct TlsInfo {
    pub version: String,                  // "TLS 1.3"
    pub cipher: String,
    pub sni: Option<String>,
    pub alpn: Option<String>,
    pub cert_subject: Option<String>,
    pub cert_issuer: Option<String>,
    pub cert_valid_from: Option<i64>,
    pub cert_valid_to: Option<i64>,
}

pub struct WsFrame {
    pub direction: WsDirection,           // ClientToServer | ServerToClient
    pub opcode: WsOpcode,                 // Text | Binary | Ping | Pong | Close
    pub payload_ref: BodyRef,
    pub timestamp: i64,
}
```

### 3.4 拦截规则模型（Intercept Rules，对标 Charles 的 Breakpoints/Map Local/Map Remote/Rewrite）

```rust
pub enum InterceptRule {
    Breakpoint { match_: RuleMatcher, on_request: bool, on_response: bool },
    MapLocal   { match_: RuleMatcher, local_file_or_body: String },
    MapRemote  { match_: RuleMatcher, target_url: String },
    Rewrite    { match_: RuleMatcher, operations: Vec<RewriteOp> }, // 增/删/改 header、body 字段替换（支持正则）
    Block      { match_: RuleMatcher },
    ThrottleOrDelay { match_: RuleMatcher, delay_ms: u64, throughput_kbps: Option<u32> },
}

pub struct RuleMatcher {
    pub host_pattern: Option<String>,     // glob，如 "*.example.com"
    pub path_pattern: Option<String>,
    pub method: Option<String>,
    pub enabled: bool,
}
```

这套规则模型是抓包代理的核心可用性来源，`cuckoo-proxy` 在自研的 `ProxyHandler::on_request`/`on_response`（见第 4 节）里按顺序匹配规则链并执行对应动作（短路返回本地内容、转发到另一个地址、暂停等待用户在 UI 里编辑后放行、丢弃、限速）。

---

## 4. MITM 代理内核设计（`cuckoo-proxy`）

### 4.1 架构原则：完全自研，不依赖第三方整体封装的 MITM 代理库

**明确决策：`cuckoo-proxy` 不使用 `hudsucker` 或任何其他第三方整体封装的 Rust MITM 代理库**。TCP 监听、协议探测、CONNECT 隧道处理、TLS 动态签发与终止、HTTP/1.1 与 HTTP/2 状态机、WebSocket 帧转发、拦截规则引擎、断点机制，全部是我们自己的代码。理由详见 `investigation.md` 2.4 节，核心是：需要深入协议链路内部的控制粒度（帧级可视化、自定义断点语义、灵活的规则介入时机）、不希望核心能力受制于一个维护活跃度不确定的小众依赖、以及自研本身能让团队吃透协议细节，为后续扩展打好基础。

底层字节级协议编解码仍复用成熟的协议库（重复造这类轮子没有价值，还会引入安全隐患）：`rustls`（TLS 握手/记录层）、`rcgen`（证书密码学操作）、`h2`（HTTP/2 帧编解码）、`tokio-tungstenite`（WebSocket 帧编解码）。但所有**行为逻辑**（转发决策、协议探测、拦截、篡改、断点、规则匹配、tunnel-in-tunnel 转发流程本身）都由 `cuckoo-proxy` 自己的代码驱动，不经过任何第三方"代理框架"的黑盒调度。

### 4.2 自研代理引擎的分层结构

```
cuckoo-proxy/
├── listener.rs      # TCP accept 循环（tokio::net::TcpListener），每个连接 spawn 一个 task
├── sniff.rs         # 协议探测：peek 连接前几个字节，区分 TLS ClientHello / 明文 HTTP / WebSocket Upgrade / 未知协议
├── connect.rs       # CONNECT 方法处理：回复 200 建立隧道，隧道内递归走 sniff → tls/http 分支（tunnel-in-tunnel）
├── tls.rs           # TLS 终止：驱动 tokio_rustls::LazyConfigAcceptor 读 ClientHello(SNI/ALPN) → 查/签发证书 → 完成握手
├── http1.rs         # 自研 HTTP/1.1 报文状态机：request-line/header/chunked-or-length-delimited body 解析，保留原始 header 顺序与大小写
├── http2.rs         # 基于 h2 crate 帧级 API 自建的 HTTP/2 状态机：SETTINGS/HEADERS/DATA/WINDOW_UPDATE 事件 → 统一内部事件
├── ws.rs            # 基于 tokio-tungstenite 的 WebSocket 帧双向转发 + 拦截钩子
├── forward.rs       # 转发到真实上游服务器（复用 cuckoo-http 的连接逻辑）
├── rule_engine.rs   # 拦截规则匹配与执行（Block/MapLocal/MapRemote/Rewrite/Throttle）
├── intercept.rs     # 断点挂起/恢复（InterceptRegistry，基于 tokio::sync::oneshot）
└── handler.rs       # 统一的 ProxyHandler trait 定义（自己的接口，不是 hudsucker 的）
```

**统一事件模型**：参考 mitmproxy 的"协议无关 Flow 状态机 + 协议特定编解码层"设计（investigation.md 2.1 节），`http1.rs`/`http2.rs` 各自把线路上的字节转换成统一的内部事件（`RequestHeadersReceived`/`RequestBodyChunk`/`RequestComplete`/`ResponseHeadersReceived`/...），再驱动同一套与协议无关的处理流程（规则匹配 → 断点判断 → 转发/短路/丢弃），这样规则引擎和断点机制的代码只写一份，不需要为 h1/h2 分别实现。

**自定义 Handler trait**（接口形态参考 hudsucker 的 `HttpHandler`/`WebSocketHandler` 做了什么，但完全是我们自己的类型定义和实现）：

```rust
pub trait ProxyHandler: Send + Sync + 'static {
    /// 收到请求头（+可能的部分body）后调用，可放行/改写/短路返回响应/挂起等待断点
    async fn on_request(&self, ctx: &FlowContext, req: HttpMessage) -> RequestAction;
    /// 收到响应后调用，可放行/改写/挂起等待断点
    async fn on_response(&self, ctx: &FlowContext, res: HttpMessage) -> ResponseAction;
    /// TLS ClientHello 到达时调用，决定是否要对该连接做 MITM（返回 false 则原样透传，不解密）
    fn should_intercept_tls(&self, sni: Option<&str>, ctx: &FlowContext) -> bool;
    /// WebSocket 逐帧回调，返回 None 表示丢弃该帧不转发
    async fn on_ws_frame(&self, ctx: &FlowContext, frame: WsFrame) -> Option<WsFrame>;
}

pub enum RequestAction {
    Forward(HttpMessage),           // 放行（可能已改写）继续转发到上游
    Respond(HttpMessage),           // 短路：直接返回给客户端，不转发（Block/MapLocal 场景）
    Pause(FlowId),                  // 挂起，等待前端断点放行/修改/丢弃
}

pub enum ResponseAction {
    Forward(HttpMessage),
    Pause(FlowId),
}
```

`CuckooProxyHandler`（`ProxyHandler` 的默认实现）内部持有 `rule_engine`、`flow_sink`（批量聚合后经 Channel 推送给前端）、`intercept_registry`（管理"暂停等待用户放行"的挂起请求）：

```rust
impl ProxyHandler for CuckooProxyHandler {
    async fn on_request(&self, ctx: &FlowContext, req: HttpMessage) -> RequestAction {
        let flow_id = ctx.flow_id;
        self.flow_sink.emit_request_started(flow_id, &req);

        // 1. 按规则链匹配：Block / MapLocal / MapRemote / Rewrite
        if let Some(resolved) = self.rule_engine.apply_request_rules(&req) {
            match resolved {
                RuleOutcome::ShortCircuit(resp) => return RequestAction::Respond(resp),
                RuleOutcome::Rewritten(req) => return self.maybe_breakpoint(flow_id, req).await,
                RuleOutcome::Unchanged => {}
            }
        }

        self.maybe_breakpoint(flow_id, req).await
    }

    async fn on_response(&self, ctx: &FlowContext, res: HttpMessage) -> ResponseAction {
        // 类似地：规则匹配 + 断点挂起 + 上报 flow_sink
        ResponseAction::Forward(res)
    }

    fn should_intercept_tls(&self, sni: Option<&str>, _ctx: &FlowContext) -> bool {
        // 按域名黑名单（如证书锁定的银行 App 域名）判断是否要透传而非 MITM
        !self.rule_engine.is_passthrough_host(sni)
    }

    async fn on_ws_frame(&self, ctx: &FlowContext, frame: WsFrame) -> Option<WsFrame> {
        self.flow_sink.emit_ws_frame(ctx.flow_id, &frame);
        Some(frame)
    }
}
```

### 4.3 TCP 层与协议探测（`listener.rs` + `sniff.rs`）

```rust
async fn accept_loop(listener: TcpListener, handler: Arc<dyn ProxyHandler>) {
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peer_addr, handler).await {
                tracing::warn!(?e, "connection handling failed");
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, peer: SocketAddr, handler: Arc<dyn ProxyHandler>) -> Result<()> {
    // 显式代理模式：先按 HTTP/1.1 报文解析第一行，判断是 CONNECT 还是绝对 URI 请求
    let first_line = peek_first_line(&mut stream).await?;
    if first_line.starts_with("CONNECT ") {
        handle_connect_tunnel(stream, &first_line, handler).await
    } else {
        // 非 CONNECT：直接按显式代理转发（明文 HTTP，无需 TLS 终止）
        handle_plain_http(stream, handler).await
    }
}

async fn handle_connect_tunnel(mut stream: TcpStream, connect_line: &str, handler: Arc<dyn ProxyHandler>) -> Result<()> {
    let target = parse_connect_target(connect_line)?;
    stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;

    // peek 隧道内的前几个字节，区分 TLS / 明文 HTTP(极少见但要兜底) / 未知协议
    let mut peek_buf = [0u8; 4];
    stream.peek(&mut peek_buf).await?;
    match &peek_buf {
        [0x16, 0x03, ..] => handle_tls(stream, target, handler).await,  // TLS record header
        b"GET " | b"POST" | b"PUT " | b"HEAD" => handle_plain_http_in_tunnel(stream, target, handler).await,
        _ => passthrough_bidirectional(stream, target).await,          // 未知协议兜底，直接透传不解析
    }
}
```

### 4.4 TLS 终止与动态证书签发（`tls.rs`）

```rust
async fn handle_tls(stream: TcpStream, target: Target, handler: Arc<dyn ProxyHandler>) -> Result<()> {
    // 只解析 ClientHello（拿到 SNI/ALPN）而不立即完成握手
    let acceptor = tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream);
    let start = acceptor.await?;
    let client_hello = start.client_hello();
    let sni = client_hello.server_name().map(String::from);

    if !handler.should_intercept_tls(sni.as_deref(), &FlowContext::new(&target)) {
        // 不拦截：把裸流还原，直接和真实服务器建立 TCP 透传（不终止 TLS，兼容证书锁定场景）
        let stream = start.into_stream_without_handshake(); // 伪代码：实际需要保留已读取的握手字节重新拼接转发
        return passthrough_bidirectional(stream, target).await;
    }

    // 拦截：查缓存或现场签发证书，构造 ServerConfig 并完成握手
    let server_config = ca.get_or_issue_server_config(sni.as_deref()).await?;
    let tls_stream = start.into_stream(server_config).await?;

    // 握手完成后，内层再走一遍完整的 HTTP 代理逻辑（tunnel-in-tunnel）
    match tls_stream.get_ref().1.alpn_protocol() {
        Some(b"h2") => http2::serve(tls_stream, target, handler).await,
        _ => http1::serve(tls_stream, target, handler).await,
    }
}
```

`ca.get_or_issue_server_config()` 内部维护一个 `DashMap<String, Arc<ServerConfig>>` 证书缓存（按域名 key），未命中时调用 `rcgen` 现场签发叶子证书（详见 4.3 节），ALPN 同时声明 `h2` 和 `http/1.1`（除非规则引擎针对该域名强制降级到 h1，参考 mitmproxy 的简化策略）。

### 4.5 断点拦截（Intercept & Resume）实现

参考 mitmproxy 的 `flow.intercept()`/`wait_for_resume()`：用 `tokio::sync::oneshot` 实现"暂停当前请求处理协程，等待前端发来放行/修改/丢弃指令"：

```rust
pub struct InterceptRegistry {
    pending: DashMap<FlowId, oneshot::Sender<InterceptDecision>>,
}

pub enum InterceptDecision {
    Continue(EditedRequestOrResponse),  // 前端可能修改过 header/body 后放行
    Abort,
    DropConnection,
}

impl InterceptRegistry {
pub async fn pause_and_wait(&self, id: FlowId, original: RequestAction) -> RequestAction {
let (tx, rx) = oneshot::channel();
self.pending.insert(id, tx);
self.flow_sink.emit_intercept_paused(id, &original);
match rx.await {
Ok(InterceptDecision::Continue(edited)) => edited.into(),
_ => RequestAction::Respond(build_dropped_response()),
}
}
}

// cuckoo-service 中的方法，唯一业务逻辑入口，通过 #[rpc_method] 宏自动暴露为 REST 端点
#[rpc_method("POST", "/api/intercepts/:id/resume")]
async fn resume_intercepted_flow(id: String, decision: InterceptDecision, service: &CuckooService) -> Result<(), ServiceError> {
    service.intercept_registry.resolve(id, decision)
}
```

### 4.6 证书体系（`cuckoo-ca`）

- 应用首次启动生成根 CA（`rcgen`），私钥+证书持久化到 OS 应用数据目录（如 macOS `~/Library/Application Support/Cuckoo/ca/`）。
- 叶子证书按域名现场签发，自建 `DashMap<String, Arc<ServerConfig>>`（或 `moka::Cache`）异步缓存，缓存未命中时调用 `rcgen` 现场签发。有效期/扩展字段（SAN、AuthorityKeyIdentifier 等）策略完全由我们自己的 `cuckoo-ca` 代码控制。
- 提供"证书安装向导"页面：导出 CA 证书文件 + 分平台安装说明（macOS Keychain / Windows certutil / Linux update-ca-certificates+NSS），v1 采用 mitmproxy 式的引导而非静默自动安装（参考 investigation.md 3.10 节的安全权衡）。
- 提供"移除 CA"功能，作为安全最佳实践的一等公民入口。

### 4.7 系统代理集成（`cuckoo-platform`）

启动代理时提供"一键设置系统代理"开关（调用分平台 shell 命令，参考 investigation.md 3.9 节），应用退出/崩溃时自动恢复。同时保留"仅监听端口，用户手动在浏览器/设备上配置代理"的路径，覆盖不希望修改全局系统设置的场景。

### 4.8 与 API 客户端引擎的关系

`cuckoo-http`（reqwest 封装）既服务于"API 客户端主动发送请求"场景，也在 MITM 代理转发请求到真实上游服务器时被复用（代理侧最终转发调用的底层 HTTP 客户端逻辑与主动发送请求共享连接池/TLS 配置代码），减少重复实现。

---

## 5. API 客户端引擎设计（`cuckoo-http` / `cuckoo-ws`）

### 5.1 请求执行

```rust
pub struct RequestExecutor {
    client: reqwest::Client,   // 复用连接池；按 Workspace 设置（代理/TLS校验）可能持有多个 Client 实例
}

pub struct ExecutionResult {
    pub response: Option<HttpMessage>,
    pub timing: FlowTiming,      // 复用 3.3 节同一套 timing 结构，两个模块共享同一可视化组件
    pub error: Option<String>,
}
```

- 变量插值：`cuckoo-templates` 提供 `{{variable}}` 语法渲染，解析链顺序为 请求级 override → Folder 继承 → Environment → Workspace 全局变量。
- 认证：内置 Basic/Bearer/ApiKey/OAuth2(Authorization Code + Client Credentials)/AWS SigV4/Digest，实现为独立可组合的"请求预处理器"，风格上类比 Yaak 的强类型 `auth-*` 扩展点（v1 内置实现，暂不做插件化，为 v2 留接口）。
- 精确计时：使用 `hyper` 底层直连或包一层自定义 `Connector`/`tower::Layer` 采集 DNS/Connect/TLS/Send/TTFB 阶段耗时，与 `FlowTiming` 结构对齐，使"抓包视图"和"主动发送请求"的耗时瀑布图 UI 组件可以复用同一套渲染代码。

### 5.2 WebSocket 客户端模式

独立于代理拦截路径，用户可以直接在"新建 WebSocket 请求"里用 `tokio_tungstenite::connect_async` 连接任意 WS 服务器，通过 Channel 收发帧，UI 展示与代理里 WebSocket Flow 详情复用同一个帧列表组件。

### 5.3 GraphQL

不做独立协议栈。GraphQL 请求类型在 UI 层是"HTTP POST 的特殊表单"（query 编辑器 + variables JSON 编辑器 + 可选的 schema 内省/自动补全），落地时序列化成标准 `RequestBody::GraphQL{query, variables, operation_name}` -> 转成 JSON body 走 `cuckoo-http` 通道。Schema 内省可选调用目标 endpoint 的标准 `__schema` introspection query 缓存结果供编辑器自动补全。

### 5.4 脚本能力（v2 特性，先设计接口占位）

v1 不做任意 pre-request/test JS 脚本（降低安全和实现复杂度），但数据模型（`HttpRequestDef.pre_request_script`/`post_response_script`）预留字段。v2 参考 Bruno 的方案，用 `rquickjs`（QuickJS 的原生 Rust 绑定）在进程内提供 WASM 级隔离的 JS 沙箱，避免像 Bruno/Yaak 那样额外起 Node 子进程的复杂度，同时保持真正的沙箱隔离（而非 `node:vm` 的弱隔离模式）。

---

## 6. 前端架构设计

### 6.1 技术栈

- **React 19 + TypeScript + Vite**
- **状态管理**：Jotai（原子化，跨组件共享的领域状态）+ TanStack Query（包装所有 `lib/api/generated.ts` 生成的 `fetch` 调用，自动处理 loading/error/缓存失效）
- **UI 组件库**：Tailwind CSS + shadcn/ui，底层 headless 交互组件用 **Base UI**（而非 Radix UI），理由见 investigation.md 4.3 节。`components/ui/` 目录视为生成后基本不做侵入式修改的区域，业务定制组件放在 `components/custom/` 另行组合；搭配 `lucide-react`（图标）、`cmdk`（命令面板）、`sonner`（Toast）、`react-hook-form`+`zod`（表单与校验）
- **表格**：**TanStack Table**（headless 逻辑层）+ `@tanstack/virtual`（虚拟化），用于抓包流量列表；行上右键菜单等交互用 Base UI 的 `ContextMenu` 叠加实现，与表格数据层独立，详见 investigation.md 4.4 节
- **树形结构**：**react-arborist**（Collection 树），内置虚拟化/拖拽排序/多选/键盘导航，详见 investigation.md 4.4 节
- **Diff 预览**：可编辑内容的 diff 场景（如 Rewrite 规则改写前后对比）用基于 Base UI 组件自建的并排视图；纯查看型的 diff（如两次 Flow 响应对比）用轻量的 `diff`+`diff2html`
- **布局**：自研可调整大小面板组件（`components/custom/ResizablePanel`），不引入 `react-resizable-panels` 等第三方布局库——固定几块区域、比例可调、支持嵌套、比例持久化，需求边界窄，自研成本低，详见 investigation.md 4.5 节；后续如确有"独立 Tab 查看 Flow 详情"等更高阶需求，优先在此基础上扩展
- **虚拟列表**：`@tanstack/virtual`（抓包流量列表、Collection 树里请求数量巨大时也可复用）
- **类型来源**：`ts-rs` 从 Rust 自动生成，前端零手写 domain 类型

### 6.2 主要界面模块

1. **顶部模式切换**：`Client`（API 客户端视角）↔ `Proxy`（抓包视角），两者共享同一侧边栏 Workspace 概念，但主工作区完全不同布局。
2. **Client 模式**：
   - 左侧：Collection 树（Folder/Request 拖拽排序，参考 Bruno/Hoppscotch 的 KeyValue 编辑器交互）
   - 中间：请求编辑器（URL 栏+Method 选择器，Tab 页：Params/Headers/Auth/Body/Scripts）
   - 右侧/下方：响应查看器（Preview/Raw/Headers/Cookies/Timing 瀑布图 Tab）
   - 顶部工具栏：环境选择下拉框（列表/树形组件选型见 6.1 节）
3. **Proxy 模式**：
   - 顶部：代理启停开关、监听端口配置、证书安装向导入口
   - 左侧：流量列表（虚拟化，列：方法/状态码/域名/大小/耗时/协议标记），支持按域名/路径/状态码筛选和搜索
   - 右侧：选中 Flow 详情（Request/Response/Timing/TLS/WebSocket 帧 Tab），修改后可"重发"或"保存为 Collection 请求"（联动 Client 模式的核心功能点）
   - 独立的"拦截规则"面板：断点列表、Map Local/Remote 配置、Rewrite 规则编辑器（用 diff 预览展示改写前后对比，方案见 6.1 节）
4. **设置**：证书管理（导出/安装引导/吊销重新生成）、系统代理开关、通用偏好（主题/字体/数据存储位置）

### 6.3 Rust → 前端高频数据流

前端**业务 API 调用**统一用标准浏览器 API 访问 `cuckoo-server`：请求-响应式操作用 `fetch`（经 `lib/api/generated.ts` 生成的强类型封装），服务端主动推送用 `EventSource` 订阅 SSE 端点。前端页面本身统一由 `cuckoo-desktop` 经 `tauri://` 加载，不存在其他加载方式，详见 2.2/2.4 节。

```typescript
// lib/api/flowStream.ts
import type { TrafficEvent } from './generated';  // ts-rs 生成的类型，与 cuckoo-flow 的 Rust 定义同源

function subscribeFlowStream(token: string, onBatch: (events: TrafficEvent[]) => void) {
  const source = new EventSource(`/api/flows/stream?token=${encodeURIComponent(token)}`);

  source.addEventListener('flow.batch', (e: MessageEvent) => {
    const batch: TrafficEvent[] = JSON.parse(e.data);   // 批量数组，而非单条
    onBatch(batch);
  });

  // EventSource 原生自动重连；配合服务端的 Last-Event-ID 支持可从断线处继续
  source.onerror = () => {
    useConnectionStore.getState().markReconnecting();
  };

  return () => source.close();
}

// 使用方
useEffect(() => subscribeFlowStream(token, (batch) => useFlowStore.getState().applyBatch(batch)), []);
```

Body 内容通过独立的 REST 端点惰性拉取（生成的强类型封装函数）：

```typescript
// 由 build.rs 从 cuckoo-service 方法清单生成
await api.flows.getBody(flowId, { part: 'request' | 'response' });
// 等价于 fetch(`/api/flows/${flowId}/body?part=...`, { headers: { Authorization: `Bearer ${token}` } })
```

鉴权 token 在应用启动时由前端主动调用一个极薄的 Tauri command `get_server_token()` 拉取（因为页面是经 `tauri://` 加载的，能直接调用 Tauri 提供的 `invoke` API），比“URL 参数/全局变量注入”这类针对普通网页的变通方案更干净（token 不会残留在 URL 或页面 HTML 里）；拿到 token 后前端在内存中保存，后续 `fetch` 请求携带在 `Authorization` 头里、`EventSource` 请求仍需拼在 `?token=` query 参数里（浏览器 `EventSource` 原生不支持自定义请求头），详见 7.5 节。

---

## 7. CLI 与 MCP Server 设计（AI 友好架构落地细节）

总体架构原则已在 2.2 节确立（Service 层 + 单一 HTTP+SSE Server，`cuckoo-cli`/`cuckoo-mcp`/桌面 UI 都作为 `cuckoo-server` 的客户端），本节展开具体的能力清单、命令/工具设计与鉴权细节。

### 7.1 设计目标

CLI 和 MCP Server 的能力范围与桌面 UI **对等**（用户明确要求"全面控制"），而不是阉割版：发送请求、管理 Collection/Environment、启停代理与查看状态、查询/订阅实时抓包流量、管理拦截规则（Breakpoint/MapLocal/MapRemote/Rewrite/Block/Throttle）、处理断点放行决策、证书导出、系统代理设置，因为桌面 UI 自身也走同一套 REST/SSE 接口（见 2.2 节），这份接口天然就是对等的，不存在"为 CLI/MCP 单独裁剪"的情况。新增任意一个能力时，唯一的强制要求是"先加到 `cuckoo-service`"，CLI/MCP 子命令/tool 是否同步暴露该能力可以按需裁剪，但底层能力入口只有一处。

### 7.2 `cuckoo-server` REST/SSE API 设计（三类客户端的共同基座）

按 Service 层的领域划分路由，均为标准 REST 语义 + 一个 SSE 订阅端点：

```
POST   /api/requests/send              # 发送一个 ad-hoc 或已保存的请求
POST   /api/requests/:id/replay        # 重放某个 Flow 的原始请求

GET    /api/workspaces                 # Workspace 列表
POST   /api/workspaces                 # 新建
GET    /api/workspaces/:id/tree        # 整棵 Collection 树（Folder+Request）
POST   /api/folders /api/requests /api/environments   # 对应 CRUD

POST   /api/proxy/start                # 启动代理（body: { port }）
POST   /api/proxy/stop
GET    /api/proxy/status               # 是否在跑、端口、已捕获 Flow 数

GET    /api/flows                      # 查询历史 Flow（支持按域名/路径/状态码/时间范围过滤+分页）
GET    /api/flows/:id
GET    /api/flows/:id/body?part=request|response   # 惰性拉取 body
GET    /api/flows/stream               # SSE：实时 Flow 事件订阅（text/event-stream）

GET/POST/PUT/DELETE  /api/rules        # 拦截规则 CRUD
GET    /api/intercepts/pending         # 当前卡在断点等待处理的请求列表
POST   /api/intercepts/:id/resume      # 放行/修改后放行/丢弃

POST   /api/certs/export               # 导出根 CA 证书
POST   /api/system/proxy               # 系统代理一键开关
```

SSE 推送的事件消息体统一复用同一个 `#[ts(export)]` 类型定义的 `TrafficEvent`（见 6.3 节）——桌面 UI、CLI、MCP 订阅的是完全相同的事件流，不存在"传输载体不同、事件模型也要各写一份"的情况。

### 7.3 `cuckoo-cli` 子命令设计

```
cuckoo send <method> <url> [--header k=v]... [--body @file|-d 'json']   # 发送一次性请求，打印响应
cuckoo request run <workspace>/<request-name> [--env <env-name>]       # 运行 Collection 里保存的请求
cuckoo collection list|tree|export <workspace>
cuckoo proxy start [--port 8899] [--system-proxy]
cuckoo proxy stop
cuckoo proxy status
cuckoo flow list [--host <glob>] [--status <code>] [--since <time>] [--follow]   # --follow 类似 tail -f，走 SSE 订阅
cuckoo flow show <flow-id> [--body request|response]
cuckoo rule add|list|rm ...
cuckoo intercept list                 # 查看当前卡住的断点请求
cuckoo intercept resume <id> [--edit-body @file] [--drop]
cuckoo cert export [--install-hint]
cuckoo server start [--headless] [--port 4173]   # 显式拉起本地 Server（不开 GUI）
```

`cuckoo` 主二进制在未检测到本地 Server 运行时，对于一次性命令（如 `cuckoo send`）自动 fork 一个短生命周期的 headless Server 完成请求后退出；对于需要持续订阅的命令（如 `cuckoo flow list --follow`、`cuckoo proxy start`），如果本地没有 Server 在跑，则提示用户先执行 `cuckoo server start` 或直接帮用户以前台/后台方式拉起，具体交互在实现阶段结合真实使用体验决定。

### 7.4 `cuckoo-mcp` Server 设计

面向 AI Agent 暴露的 MCP tools（用 `rmcp` 实现，进程内可直连 `cuckoo-service`，跨机器/沙箱场景走 `cuckoo-server` 的 HTTP API）：

| MCP Tool | 对应能力 | 典型使用场景 |
|---|---|---|
| `send_request` | 发送一次性 HTTP 请求 | "帮我测一下这个接口返回什么" |
| `list_collections` / `run_saved_request` | Collection 查询与执行 | "把 Collection 里那个登录接口跑一遍" |
| `start_proxy` / `stop_proxy` / `get_proxy_status` | 代理生命周期管理 | "帮我开代理抓一下这个 App 的包" |
| `list_flows` / `get_flow_detail` | 查询/检索抓包记录 | "看看刚才有没有请求返回了 500" |
| `create_rewrite_rule` / `create_map_local_rule` | 拦截规则管理 | "帮我把这个接口的响应改成本地这个 mock 文件" |
| `list_pending_intercepts` / `resume_intercept` | 断点处理 | "这个卡住的请求该不该放行，帮我看看 body 对不对" |

MCP transport 优先支持 **stdio**（本地 AI IDE/Agent 场景，如本工具自身）和 **Streamable HTTP**（远程/多客户端场景），两种 transport 背后调用同一套 tool 实现，不重复写业务逻辑。

### 7.5 安全边界

`cuckoo-server` 默认只绑定 `127.0.0.1`，启动时在应用数据目录写入/复用一个随机 token 文件（`server.token`），所有 REST 请求与 SSE 订阅均需在 `Authorization: Bearer <token>` 请求头中携带该 token；`cuckoo-cli`/`cuckoo-mcp` 启动时自动读取该文件，无需用户手工配置。浏览器 `EventSource` 原生不支持自定义请求头，因此 SSE 端点额外接受 `?token=` 查询参数作为兼容手段（详见 `investigation.md` 3.14 节），但 REST 端点一律只认 `Authorization` 头，不接受 URL 携带 token。

**桌面 UI 的 token 获取方式**：桌面 UI 的页面是通过 Tauri 的 `tauri://` 协议加载的，前端可以直接调用 Tauri 提供的 `invoke` API，因此用一个极薄的 `get_server_token()` Tauri command 在启动时主动拉取 token（不落地到 URL 或页面 HTML，是三类客户端里最干净的一种拿 token 方式）。由于前端页面仅通过 Tauri 加载、不存在其他加载途径，也就不存在"非桌面场景的页面鉴权"这个问题。MCP Server 若以 Streamable HTTP 暴露给非本机的远程调用方，则必须提示用户额外确认（默认不建议绑定非 loopback 地址），避免代理证书私钥、Collection 中可能存储的密钥信息被跨网络访问。

**桌面场景引入的一个新技术细节：跨源请求（CORS）**——由于页面通过 `tauri://` 加载、而业务 API 请求发往 `http://127.0.0.1:<port>`，这在浏览器同源策略下属于跨源请求（`tauri://` 与 `http://127.0.0.1` 是不同源），`cuckoo-server` 的 CORS 中间件需要显式放行来自 `tauri://` 源的 `fetch`/`EventSource` 请求（Tauri 2.x 下 `WebviewWindow` 发起请求时的 `Origin` 头形如 `tauri://localhost`），否则请求会被浏览器内核拦截。这是把"页面加载"和"业务 API"拆成两条通道后新增的配置项，`auth.rs` 的 Origin 校验中间件需要同时维护一份"允许的 Origin 列表"（`tauri://localhost` + 本机 loopback 若干形式），不需要历经其他来源（cuckoo-server 不面向局域网/公网开放）。

---

## 8. 范围界定与风险控制（v1 边界）

| 能力 | v1 范围 | 说明 |
|---|---|---|
| HTTP/1.1 请求发送 | ✅ | |
| HTTP/2 请求发送 | ✅ | |
| HTTP/3 请求发送 | 🟡 Beta | 依赖 `h3`/`h3-quinn` 0.0.x 不稳定 API |
| WebSocket 客户端 | ✅ | |
| GraphQL 请求（含 introspection） | ✅ | 无独立协议栈，UI 层特化 |
| gRPC | ⏳ v2 | 参考 Yaak 的 tonic+prost-reflect 方案 |
| MITM 拦截 HTTP/1.1 | ✅ | |
| MITM 拦截 HTTP/2 | ✅ | |
| MITM 拦截 HTTP/3 (QUIC) | ❌ 明确排除 | 见 investigation.md 3.6 节技术风险分析 |
| MITM 拦截 WebSocket | ✅ | |
| 断点拦截 (Intercept & Resume) | ✅ | |
| Map Local / Map Remote / Rewrite / Block / Throttle | ✅ | |
| 证书安装 | ✅ 引导式（v1）| 一键提权安装列为 v2 加分项 |
| 系统代理一键设置 | ✅ | 分平台 shell 胶水代码 |
| 移动设备/Docker 免配置抓包 | ❌ v1 不做 | 远期可参考 httptoolkit-server 的 `interceptors/` 设计逐步补齐 |
| Pre-request/Test 脚本 | ⏳ v2 | 预留数据模型字段，v2 用 `rquickjs` 实现 |
| Collection 导入（Postman/Insomnia/OpenAPI）| ⏳ v2 | 有明确的开源参考实现（`postman-collection`、Bruno/Hoppscotch 的 converters）|
| Git 友好的纯文本导出格式 | ⏳ v2 | 参考 Bruno `.bru`/OpenCollection YAML 理念，作为 SQLite 主存储之外的导出选项 |
| 团队协作/云同步 | ❌ 不做 | local-first 定位 |
| 本地 HTTP+SSE Server（`cuckoo-server`） | ✅ | 见第 7 章，M1 起随核心能力同步暴露 REST/SSE 端点 |
| CLI（`cuckoo-cli`） | ✅ | 见 7.3 节 |
| MCP Server（`cuckoo-mcp`） | ✅ | 见 7.4 节，v1 先支持 stdio transport，Streamable HTTP 可延后 |

具体阶段划分与里程碑详见 `plan.md`。
