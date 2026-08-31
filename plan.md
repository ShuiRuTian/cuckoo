# 实施计划：Cuckoo

> 基于 `spec.md` 的分阶段实施路线图。每个阶段结束都应该有一个可运行、可演示的产物（Demoable Milestone），而不是"写了一堆代码但跑不起来"。

---

## 阶段总览

| 阶段 | 目标 | 核心产出 |
|---|---|---|
| M0 | 项目脚手架 | Tauri + React 项目可运行，空壳 UI |
| M1 | API 客户端 MVP | 能发送 HTTP 请求、管理 Collection、查看响应 |
| M2 | MITM 代理 MVP | 能起代理、生成安装 CA、拦截明文+HTTPS 流量并展示 |
| M3 | 拦截规则与联动 | 断点/Map Local/Rewrite、抓包→Collection 联动 |
| M4 | WebSocket 全链路 | 客户端 WS 连接 + 代理 WS 拦截 |
| M5 | 打磨与 v1 收尾 | 性能优化、证书安装引导完善、易用性打磨 |
| v2+ | 后续增强 | gRPC、脚本引擎、导入导出、HTTP/3、移动端免配置抓包 |

---

## M0：项目脚手架

**目标**：`pnpm tauri dev` 能跑起来，展示一个空的双模式（Client/Proxy）布局框架；`crates/`（Tauri-free 核心 + Service 层）与 `apps/`（`cuckoo-desktop`/`cuckoo-server`/`cuckoo-cli`/`cuckoo-mcp` 四个平级入口，均只是 `cuckoo-server` 的调用方或壳）的目录骨架从第一天就按 `spec.md` 2.1 节的最终形态搭好，避免后续大规模目录迁移。

- [x] 初始化 Cargo workspace，按 `spec.md` 2.1 节建立顶层目录：`crates/cuckoo-core`、`cuckoo-store`、`cuckoo-http`、`cuckoo-ws`、`cuckoo-proxy`、`cuckoo-ca`、`cuckoo-flow`、`cuckoo-platform`、`cuckoo-templates`、`cuckoo-service`（先建目录+`Cargo.toml`+`lib.rs` 占位，接口留空）
- [x] 实现最小版 `#[rpc_method(METHOD, PATH)]` 属性宏（`spec.md` 2.3 节）：编译期把标注的 Service 方法登记进一张全局方法清单，先只做到"能收集方法签名 + 自动拼装 `axum::Router`"，TS 类型生成/CLI/MCP 派生可以后续逐步接入
- [x] 建立 `apps/cuckoo-server`（`axum` 骨架，只提供业务 API，不承担任何静态文件/前端页面托管职责）：监听 `127.0.0.1`，接入 `auth.rs` 的 `Authorization: Bearer <token>` 鉴权中间件（token 文件生成/读取）与 CORS/Origin 校验中间件（放行 `tauri://localhost` 等 Tauri 页面源发起的跨源请求，见 `spec.md` 7.5 节）、`sse.rs` 的 SSE 端点骨架；建立 `apps/cuckoo-desktop`（用 `create-tauri-app` 初始化 Tauri 2.x + React + TypeScript + Vite），`main.rs` 启动时 `tokio::spawn` 拉起 `cuckoo-server` 供业务 API 使用，`WebviewWindow` **始终沿用 Tauri 默认的 `tauri://` 自定义协议加载打包进二进制的前端静态资源，这是前端页面唯一的加载方式**（不要改成加载 `http://127.0.0.1:<port>/`，也不要给 `cuckoo-server` 添加任何静态文件托管能力，保留 Tauri 官方资源加载机制的性能与安全优势，见 `spec.md` 2.2/2.4 节），页面内前端代码用标准 `fetch`/`EventSource` 访问同一个 `cuckoo-server` 端口；`system_commands.rs` 实现 `get_server_token()` Tauri command 供前端启动时拉取鉴权 token；`apps/cuckoo-cli`、`apps/cuckoo-mcp` 先建最小可编译骨架（`main.rs` 打印 help 即可），具体命令/工具留到 M5 集中实现（见 M5 新增的 “CLI 与 MCP 落地” 小节）
- [x] 引入前端基础依赖：Jotai、TanStack Query、Tailwind CSS + shadcn/ui（基于 Base UI）、TanStack Table、react-arborist、`@tanstack/virtual`（不引入 `react-resizable-panels`，布局用自研组件，见下一条）
- [x] 实现自研可调整大小面板组件（`components/custom/ResizablePanel`）最简版本：水平/垂直切分、拖拽 divider 调整比例即可，嵌套/持久化/键盘可访问性可以后续迭代补齐
- [x] 搭建顶层布局：顶部模式切换 Tab（Client / Proxy）+ 左侧边栏占位 + 主工作区占位（用上一条的自研面板组件搭建分栏）
- [x] 配置 `ts-rs`：验证一个简单 Rust struct 能自动生成 TS 类型文件并被前端正确 import
- [x] 打通最小闭环：`cuckoo-service` 里写一个 `ping()` 方法并用 `#[rpc_method("GET", "/api/ping")]` 标注，`cuckoo-server` 自动生成对应路由；前端先通过 `get_server_token()` Tauri command 拿到 token，再用 `fetch` 携带该 token 调用 `/api/ping` 并在页面上显示返回值——验证"写一次 Service 方法，自动出现 REST 端点"以及"页面经 tauri:// 加载、业务请求经 cuckoo-server"这两条链路端到端可用

**验收标准**：应用能启动，能看到 Client/Proxy 两个空白 Tab（用自研可调整大小面板搭建），点击按钮能通过 `fetch` 调用 `/api/ping` 并在前端看到返回值；`cargo build --workspace` 能编译通过 `apps/` 下四个 crate。

---

## M1：API 客户端 MVP

**目标**：像一个最小可用的 Postman —— 能新建请求、发送、看响应、存到 Collection 里。

### 1.1 数据层
- [x] `cuckoo-store`：接入 `sea-orm`（`sqlx-sqlite` 驱动 + `runtime-tokio-rustls`），用 `DeriveEntityModel` 定义 Workspace/Folder/HttpRequestDef/Environment 四个 Entity 及其 `Related` 关联关系
- [x] 接入 `sea-orm-migration`，编写首个版本的建表迁移（代替手写 SQL 迁移脚本），应用启动时自动执行待应用的迁移
- [x] 实现基本 CRUD 的 Rust 函数（不含 `#[rpc_method]` 包装，纯数据层逻辑）
- [x] 单元测试：建表、插入、查询、级联删除

### 1.2 请求执行引擎
- [x] `cuckoo-http`：封装 `reqwest::Client`，实现 `RequestExecutor::execute()`，支持 method/url/headers/query params/body（Raw JSON 起步，其他 body 类型后续补）
- [x] `cuckoo-templates`：实现 `{{variable}}` 插值渲染 + 简单的变量解析链（先只做 Environment 级，不做 Folder 继承）
- [x] 计时数据采集（先做粗粒度：total time，DNS/TLS 精细阶段可以放到 M5 打磨阶段）

### 1.3 Service 层方法与 REST 端点
- [x] `cuckoo-service::collection_service`：Workspace/Folder/Request/Environment 的增删改查方法，各自标注 `#[rpc_method]` 暴露为 REST 端点（`POST/GET/PUT/DELETE /api/workspaces` 等，见 `spec.md` 7.2 节）
- [x] `cuckoo-service::request_service`：`send_request(request_id | ad_hoc_request)` 方法，标注为 `POST /api/requests/send`，返回 `ExecutionResult`
- [x] 构建脚本雏形：遍历 `#[rpc_method]` 收集到的方法清单，为前端生成 `lib/api/generated.ts` 强类型 `fetch` 封装函数（先手写一份简单的清单式 `build.rs` 即可，见 `spec.md` 2.3 节）

### 1.4 前端
- [x] Collection 树组件（基于 react-arborist，新建/重命名/删除 Folder 和 Request，暂不做拖拽排序）
- [x] 请求编辑器：URL 栏 + Method 选择器 + Params/Headers/Body(Raw) Tab
- [x] 响应查看器：状态码/耗时/Headers/Body（JSON 自动美化展示，具体文本/代码编辑器方案待定）
- [x] 环境变量管理界面（简单的 Key-Value 列表 + 环境切换下拉框）
- [x] 用生成的 `lib/api/generated.ts` 封装 + TanStack Query 接入以上界面的数据读写，鉴权 token 通过前端主动调用 `get_server_token()` Tauri command 拉取（见 `spec.md` 6.3 节）

**验收标准**：新建一个 GET 请求打 `https://httpbin.org/get`，能看到响应 JSON；新建一个 Environment 定义 `baseUrl` 变量，请求 URL 用 `{{baseUrl}}/get` 能正确替换发送。

---

## M2：MITM 代理 MVP

**目标**：启动代理监听端口，配置系统代理后能看到浏览器请求实时出现在流量列表里，包括 HTTPS 流量（需要安装并信任 CA）。

### 2.1 证书体系
- [x] `cuckoo-ca`：应用首次启动生成根 CA（`rcgen`），持久化到应用数据目录
- [x] `cuckoo-service::system_service::export_ca_cert()` 方法，标注 `#[rpc_method]` 暴露为 `POST /api/certs/export`，前端提供下载按钮
- [ ] 编写分平台安装说明文案（macOS/Windows/Linux），先不做自动化一键安装

### 2.2 代理内核（完全自研，不依赖 `hudsucker` 等第三方 MITM 代理封装库，详见 spec.md 第 4 节）
- [x] `cuckoo-proxy/listener.rs`：`tokio::net::TcpListener` accept 循环，每个连接 spawn 一个 task
- [x] `cuckoo-proxy/connect.rs`：解析 CONNECT 请求行，回复 `200 Connection Established` 建立隧道
- [ ] `cuckoo-proxy/sniff.rs`：隧道内 peek 前几个字节区分 TLS ClientHello（`0x16 0x03`）/ 明文 HTTP / 未知协议兜底透传
- [x] `cuckoo-proxy/tls.rs`：用 `tokio_rustls::LazyConfigAcceptor` 解析 ClientHello 拿到 SNI，接入 2.1 节 CA 现场签发证书并完成握手
- [x] `cuckoo-proxy/http1.rs`：自研 HTTP/1.1 报文状态机（request-line/header/chunked 或 Content-Length body 解析），保留原始 header 顺序与大小写；先只做日志打印，不做规则匹配
- [x] `cuckoo-proxy/forward.rs`：把解析出的请求转发到真实上游服务器（复用 `cuckoo-http` 的连接逻辑），拿到响应后按 HTTP/1.1 写回客户端
- [x] `cuckoo-proxy/handler.rs`：定义自己的 `ProxyHandler` trait（`on_request`/`on_response`/`should_intercept_tls`），先给一个只打日志的默认实现
- [x] `cuckoo-service::proxy_service`：`start_proxy(port)` / `stop_proxy()` 方法，标注 `#[rpc_method]` 暴露为 `POST /api/proxy/start` / `POST /api/proxy/stop`，内部用 `tokio::spawn` 跑 accept 循环
- [ ] 验证明文 HTTP 和 HTTPS（安装 CA 后）都能被正确拦截转发，不影响正常上网
- [ ] （可选，若时间允许提前做）`cuckoo-proxy/http2.rs`：基于 `h2` crate 帧级 API 实现 HTTP/2 状态机，ALPN 协商 `h2` 时走此分支；时间不够可推迟到本阶段末尾或 M5

### 2.3 Flow 事件管道
- [x] `cuckoo-flow`：定义 `Flow` 数据结构（先做精简版：不含 TLS 详情/WS 帧，聚焦 request/response/timing）
- [x] 实现批量聚合器：内部 `mpsc` channel 收集 handler 产生的事件，16-50ms 窗口聚合后通过 `tokio::sync::broadcast` 对外暴露订阅接口
- [x] `cuckoo-server/sse.rs`：`GET /api/flows/stream` SSE 端点，订阅上述 `broadcast` channel，把批量事件序列化为 `flow.batch` SSE 消息推送给所有连接的客户端（桌面 UI/CLI/MCP/浏览器共用同一端点）
- [x] Body 惰性拉取方法：`cuckoo-service::proxy_service::get_flow_body(flow_id, part)`，标注为 `GET /api/flows/:id/body`
- [x] 前端 `lib/api/flowStream.ts`：封装 `EventSource` 订阅逻辑（含自动重连提示），供 Proxy 模式界面使用

### 2.4 系统代理集成
- [x] `cuckoo-platform`：实现 macOS 的 `networksetup` 分支（优先做，因为是主要开发环境）；Windows/Linux 分支可以先留 TODO stub
- [x] "一键开启系统代理"开关 + 应用退出时自动恢复的 hook

### 2.5 前端
- [x] Proxy 模式主界面：启停开关、端口配置、证书安装引导入口
- [x] 流量列表（基于 TanStack Table + `@tanstack/virtual` 虚拟化）：方法/域名/路径/状态码/耗时列
- [x] Flow 详情面板：Request/Response 的 Headers 和 Body 展示

**验收标准**：打开系统浏览器，设置系统代理指向本应用监听端口，安装信任 CA 后，访问任意 HTTPS 网站，能在流量列表里实时看到请求记录，点击能看到完整 headers/body。

---

## M3：拦截规则与两大模块联动

**目标**：补齐 Charles/Reqable 级别的实用功能——断点修改、Map Local、Rewrite，以及"抓包记录一键转发送请求"的核心联动体验。

### 3.1 拦截规则引擎
- [x] `cuckoo-proxy` 新增 `RuleEngine`：实现 `RuleMatcher`（host/path glob 匹配）
- [x] 实现 `Block`、`MapLocal`、`MapRemote`、`Rewrite` 四种规则的执行逻辑
- [x] 实现断点 `Breakpoint`：`InterceptRegistry` + `oneshot` 挂起等待机制
- [x] `cuckoo-service::rule_service`：规则的增删改查方法（`#[rpc_method]` 暴露为 `/api/rules` CRUD）、`resume_intercepted_flow(id, decision)`（暴露为 `POST /api/intercepts/:id/resume`，见 `spec.md` 4.5 节示例）

### 3.2 前端拦截规则 UI
- [x] 规则列表管理界面（新建/启用禁用/排序/删除）
- [x] 断点命中时的模态编辑界面：展示原始 request/response，允许编辑 headers/body 后放行，或直接丢弃/中断连接
- [x] Rewrite 规则的 diff 预览（基于 Base UI 自建并排视图，展示修改前后对比）

### 3.3 联动功能
- [x] "另存为 Collection 请求"：从任意 Flow 详情面板一键把 request 转成 `HttpRequestDef` 存入指定 Workspace/Folder
- [x] "重新发送"：直接用 `cuckoo-http` 重放某个 Flow 的原始请求（可先编辑再发）
- [x] Collection 请求发送时也能选择"经过本地代理转发"（方便调试请求本身在代理规则下的行为）

**验收标准**：配置一条 Rewrite 规则给所有到 `api.example.com` 的请求加一个自定义 header，验证生效；命中一个断点规则后能在 UI 里修改 body 再放行，验证目标服务器收到的是修改后的内容；从流量列表选中一条记录另存为 Collection 请求并成功重发。

---

## M4：WebSocket 全链路

**目标**：无论是主动新建 WS 连接，还是代理里拦截到的 WS 升级流量，都能看到逐帧收发消息。

- [ ] `cuckoo-ws`：封装 `tokio_tungstenite`，实现主动连接/发送帧/接收帧的 Service 方法（`#[rpc_method]` 暴露连接管理 REST 端点），逐帧事件通过 SSE 推送给前端
- [ ] `cuckoo-proxy/ws.rs`：识别 HTTP Upgrade 握手后，用 `tokio-tungstenite` 做双向帧编解码，自研转发循环并在其中插入 `ProxyHandler::on_ws_frame` 拦截钩子，捕获逐帧数据写入 `Flow.websocket_frames`
- [ ] 前端：WS 帧列表组件（方向/opcode/payload/时间戳），Client 模式下的"新建 WebSocket 请求"面板（连接地址栏+发送框+帧历史），复用同一套帧列表 UI 组件
- [ ] GraphQL Subscription（`graphql-ws` 协议）识别与友好展示（可选加分项，若时间不够可推迟到 v2）

**验收标准**：新建一个 WS 请求连接 `wss://echo.websocket.org`，发送消息能看到回显；配置代理拦截一个使用 WebSocket 的网页，能看到逐帧消息记录。

---

## M5：打磨与 v1 收尾

**目标**：补齐 v1 范围内被简化/延后的细节，提升稳定性和易用性，达到"能日常真实使用"的水准。

- [ ] 精细计时：DNS/Connect/TLS/Send/TTFB 阶段耗时采集，瀑布图可视化组件（Client 和 Proxy 两个模块复用同一组件）
- [ ] TLS 详情面板（协议版本/密码套件/证书链信息）
- [ ] Windows / Linux 的系统代理设置分支补齐并测试
- [ ] CA 安装引导页面打磨（分平台图文说明，或提供一键脚本按钮）+ "移除 CA" 功能
- [ ] 大流量场景性能测试与优化：环形缓冲上限、body 惰性加载验证、批量聚合参数调优
- [ ] 认证方式补全：OAuth2/AWS SigV4/Digest（M1 只做了 Basic/Bearer/ApiKey 的话）
- [ ] Body 类型补全：FormData/UrlEncoded/Binary（M1 只做了 Raw 的话）
- [ ] 请求/Collection 树的拖拽排序
- [ ] 应用图标、基础主题（深色/浅色）、快捷键
- [ ] 崩溃恢复：应用异常退出后自动清理系统代理设置的兜底逻辑验证
- [ ] 打包与分发：`tauri build` 产出 macOS/Windows/Linux 安装包，签名/公证（至少 macOS，涉及系统信任库写入操作对代码签名要求更敏感）

### 5.1 CLI 与 MCP 落地（基于已有 REST/SSE API 集中实现，见 `spec.md` 第 7 章）
- [ ] `apps/cuckoo-cli`：实现 `spec.md` 7.3 节列出的子命令（`send`/`request run`/`collection`/`proxy start|stop|status`/`flow list --follow`/`flow show`/`rule`/`intercept`/`cert export`/`server start`），均作为 `cuckoo-server` 的 HTTP/SSE 客户端实现；`--follow`/`proxy start` 等需持续订阅的命令接入 `EventSource`等价的 Rust SSE 客户端
- [ ] 未检测到本地 Server 运行时的自动拉起逻辑（一次性命令 fork headless Server 子进程执行完退出）
- [ ] `apps/cuckoo-mcp`：基于 `rmcp` 实现 `spec.md` 7.4 节表格中的 MCP tools，优先支持 stdio transport（进程内可直连 `cuckoo-service` 或走 `cuckoo-server` HTTP 接口），Streamable HTTP transport 可延后
- [ ] 鉴权链路验证：`cuckoo-cli`/`cuckoo-mcp` 启动时自动读取 `server.token` 并携带 `Authorization: Bearer` 请求头，桌面 UI 验证 `get_server_token()` Tauri command 能正确返回 token 并被前端正确携带，验证未携带/错误 token 时被服务端拒绝

**验收标准**：作为一个真实的日常开发调试工具可用一整天不崩溃、不遗留脏系统代理设置；`cuckoo-cli send`、`cuckoo-cli flow list --follow` 能在不开启桌面 UI 的情况下独立完成发请求/订阅抓包流量；通过 `cuckoo-mcp` 的 stdio transport 能让 AI Agent 成功调用 `send_request`/`list_flows` 等至少 3 个核心 tool。

---

## v2+ 路线图（不在当前实施范围内，留作后续规划）

按 investigation.md 和 spec.md 中标记的 v2 特性，大致优先级排序：

1. **Collection 导入**（Postman/Insomnia/OpenAPI）—— 有明确参考实现（`postman-collection` npm 包、Bruno/Hoppscotch 的 converters 包），降低用户迁移成本，投入产出比高。
2. **Pre-request/Test 脚本引擎**（`rquickjs` 沙箱）—— 对标 Postman/Bruno 的核心生产力功能。
3. **gRPC 支持**（`tonic` + `prost-reflect`，参考 Yaak 实现）。
4. **Git 友好的纯文本导出格式**（参考 Bruno `.bru`/OpenCollection YAML），作为 SQLite 主存储之外的可选导出，服务团队协作场景但不做云同步后端。
5. **HTTP/2 帧级可视化**（旁路用 `h2` crate 解析帧）。
6. **移动设备免配置抓包**（参考 mitmproxy_rs 的 WireGuard 模式思路，用 `GotaTun` 自研，避免依赖 PyO3 强耦合的原始实现）。
7. **HTTP/3 (QUIC) MITM 拦截**——独立立项，工作量大，技术风险高（见 investigation.md 3.7 节），需要专门的时间预算。
8. **Frida 反 SSL Pinning 集成**（针对做了证书锁定的原生 App）。
9. **Docker 容器流量拦截**（参考 httptoolkit-server 的 Docker daemon API 代理 + 自建 DNS 方案）。

---

## 风险与依赖关系提醒

- **M2 是全项目技术风险最集中、工作量也最容易被低估的阶段**。由于 MITM 代理内核明确不使用 `hudsucker` 等第三方整体封装库（见 spec.md 第 4 节），2.2 节里 TCP accept、协议探测、CONNECT 隧道、TLS 动态签发/终止、HTTP/1.1 状态机全部要自己实现并保证正确性——这比"接入一个现成库"要多花不少时间去处理各种边界情况（如 chunked encoding 的畸形分片、HTTP/1.0 与 1.1 的差异、keep-alive 连接复用、TLS 握手失败的降级处理等）。加上 TLS 动态证书签发、系统代理配置、CA 信任安装本身就涉及操作系统证书信任链的细节排查，建议给 M2 预留比表面任务清单看起来更充分的时间，并且**允许先做一个只支持最基本场景（无 keep-alive、无 chunked、只支持 GET/POST）的最小可用版本跑通验收标准，再逐步补齐边界情况**，而不是一开始就追求完整的协议正确性。
- M1 和 M2 在数据模型上有意共享 `HttpMessage`/计时结构（见 spec.md 3.2/3.3 节），实现时注意不要让两个模块的类型定义分叉，否则后续 M3 的"抓包→Collection"联动功能会需要额外的转换层。
- 每个阶段结束后建议实际跑一遍验收标准里的手动测试场景，而不是只看单元测试通过——这类系统级/网络级功能的很多问题只在真实环境（真实浏览器、真实证书信任store）里才会暴露。
