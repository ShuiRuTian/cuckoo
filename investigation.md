# 技术调研报告：Cuckoo（Reqable 类应用）

> 本文档是项目启动前的技术调研记录，覆盖了对标产品的分析，以及 Rust/Tauri 生态中可用于实现的库和方案。**产品形态/功能范围层面的直接竞品是 Reqable**（闭源商业软件，第 0.1 节基于官网/文档做黑盒调研）；**架构与具体实现层面的参考对象**是若干开源项目源码级分析（Yaak、Bruno、Hoppscotch、mitmproxy、HTTP Toolkit/mockttp、hudsucker 等）。两类调研性质不同但互补：前者回答"要做成什么样"，后者回答"具体怎么写代码"，共同用于支撑后续 `spec.md`（产品与架构规格）和 `plan.md`（实施计划）的决策依据。
>
> **重要架构原则**：MITM 代理内核**不使用任何第三方整体封装库**（如 `hudsucker`），核心逻辑（TCP 监听、协议探测、CONNECT 隧道处理、TLS 动态签发与终止、HTTP/1.1 与 HTTP/2 状态机、WebSocket 帧转发、拦截规则引擎、断点机制）全部自行实现，原因见第 2.5 节。本文档中出现的 `hudsucker` 相关内容，定位是**架构设计参考与思路验证**（证明某种方案在 Rust 生态里跑得通、API 该怎么设计），而非依赖对象。TLS(`rustls`)、证书生成(`rcgen`)、HTTP/2 帧(`h2`)、WebSocket 帧编解码(`tokio-tungstenite`) 这类**底层协议库**仍会使用——手写 TLS 握手或 WS 帧格式解析没有额外价值，重复造这类轮子只会引入安全隐患；但这些库只负责最底层的字节级编解码，**代理的行为逻辑（转发决策、拦截、篡改、断点、规则匹配）完全由我们自己的代码驱动**。
>
> 调研日期：2026-08

---

## 0. 项目目标回顾

做一个类似 **Reqable** 的桌面应用，同时具备两类能力：

1. **API 客户端**（对标 Postman / Hoppscotch / Bruno / Yaak）：手工构造并发送 HTTP/GraphQL/WebSocket/gRPC/SSE 请求，管理集合（Collection）、环境变量、脚本等。
2. **MITM 抓包调试代理**（对标 Charles / Fiddler / HTTP Toolkit / mitmproxy）：作为中间人拦截、展示、修改任意客户端（浏览器/手机 App/curl）与服务器之间的 HTTP/1.1、HTTP/2、HTTP/3(QUIC)、WebSocket、GraphQL 流量。

技术约束：使用 **Tauri**（Rust 后端 + Web 前端）。

---

## 0.1 核心对标产品：Reqable —— 产品形态的直接竞品（闭源，黑盒调研）

**必须明确一个定位上的区分**：本文档第 1 节详细分析的 Yaak/Bruno/Hoppscotch，以及第 2 节的 mitmproxy/HTTP Toolkit，是**架构与实现层面的参考对象**（开源、有源码可读，回答"某个具体机制该怎么写代码"）；而 **Reqable 才是产品形态、功能范围、定位层面真正对标的直接竞品**——我们要做的就是一个"Reqable 那样的东西"，同时用自己的技术栈重新实现。Reqable 是**闭源商业软件**（用 Flutter/Dart 编写，非 Tauri，无公开源码），因此以下内容基于官网/官方文档/产品行为的黑盒调研，而非源码阅读，性质与本文档其他章节不同，需要单独说明。

### 0.1.1 产品定位与整体范围

Reqable 官网的自我定位极其精炼：**"Reqable = Fiddler + Charles + Postman"**，"Minimalist Design, Powerful Features, Efficient Performance and Desktop + Mobile Platforms"——可以直接作为我们的电梯演讲话术参考。它明确划分两大核心能力（与我们 `spec.md` 的模块划分一致）：

1. **API Debugging**（对应我们的 MITM 抓包模块）：经典 MITM 方式抓包，**桌面端走系统代理，移动端走 VPN 隧道**（与我们 investigation.md 2.3/3.10 节的结论完全一致）。
2. **API Testing**（对应我们的 API 客户端模块）：REST 请求编辑发送、Collection 管理、环境变量、云同步。

两大能力**双向打通**——这是 Reqable 明确宣传的差异化卖点，也是我们必须做到的产品体验，不能做成"两个粘在一起的独立工具"：
- 可以直接从**抓到的流量**一键"另存为 API"进入 Collection 管理（`Save to API Collection`），
- 也可以在**测试 API 时**同时抓包查看真实线路上发送的字节（`Bind Debugging Proxy` —— 让 API Testing 模块发出的请求也经过本地代理内核记录一份 Flow）。

**平台矩阵**：Windows/macOS/Linux 桌面 + Android/iOS 移动，而且移动端是"能力受限的独立 App"而非仅仅"桌面的遥控器"——这一点后面单独展开。

**极致的轻量化是其重要卖点之一**：官网首页专门用 benchmark 图表宣传启动时间、安装体积（~30MB）、内存占用相比 Postman/Charles 等竞品的优势。这直接支撔了我们"用 Tauri（WebView 复用系统内核，无需内置 Chromium）而非 Electron"的选型——**减小体积、降低内存/启动时间，是这个品类里被验证过的、用户真正在意的差异化点**，不是我们想当然加的约束，而是要在实现全程（尤其是前端依赖选择、Rust 侧避免过度引入重量级 crate）持续贯彻的产品级要求。

### 0.1.2 API Debugging（MITM 模块）功能清单 —— 我们的对标基准线

从官方文档目录结构（`docs/capture/`）反推出的功能全景，按我们已有章节归类：

| 功能分类 | 具体条目 | 与本文档已有调研的对应关系 |
|---|---|---|
| 协议支持 | HTTP/1.x, HTTP/2（**HTTP/3 明确未支持**）, WebSocket, SSE, TLS 1.1/1.2/1.3, IPv4/IPv6 | 与我们 3.7 节"HTTP/3 v1 不做"的判断完全一致——**连 Reqable 这样成熟的商业竞品，MITM 拦截也没做 HTTP/3**，这是对我们决策的有力佐证，而非我们能力不足的妥协 |
| 代理协议 | 单端口自适应 HTTP/HTTPS/Socks4/Socks4a/Socks5（**默认端口 9000**） | 与 HTTP Toolkit 的 "Combo Server"（2.2 节）思路一致，我们的 peek 探测+多协议适配设计已覆盖 |
| 规则引擎 | **Rewrite**（Map Remote/Map Local）、**Breakpoint**、**Python 脚本**（`onRequest`/`onResponse`）、**Gateway**（L4/L7 流量控制，见下方展开） | 我们 spec.md 已规划 Rewrite/Breakpoint/RuleEngine，**脚本引擎选型上 Reqable 选的是真 Python3（用户本机环境）而非嵌入式沙箱**，这是与我们计划用 `rquickjs`/JS 沙箱的路线**明确不同**的选择，见 0.1.4 节详细分析 |
| 高级能力 | Access Control（代理访问控制）、Mirror（流量镜像转发）、Diff（请求/响应对比）、Network Throttling（限速模拟弱网）、Reverse Proxy、Repeat（重放）、Compose（手工构造请求注入 Flow）、HAR 导入导出、**Charles `.chls` 会话文件兼容**、Turbo Mode（见下）、View-As（自定义内容查看器） | 除 Diff、Charles 会话兼容、Turbo Mode 外均已规划覆盖，这三项是需要补充进 `spec.md` 的功能缺口 |
| 移动协作 | 见 0.1.3 节 | 验证了"手机端配套 App 做进程归因"方案的可行性 |
| 代码生成/查看 | Code Generator（生成各语言请求代码）、Traffic Source Detection、多种展示视图 | 与 Postman 系"Copy as cURL/代码片段"能力对应，我们已规划 |

**Turbo Mode**：官方文档原话是"in turbo mode, **the traffic list will not be updated**, but Gateway/Mirror/Script/Rewrite/Breakpoint still work. This mode helps to keep the system resource usage low"——这说明 Reqable 把"**代理转发引擎**"和"**Flow 落地到可视化列表（UI 状态同步/持久化）**"在架构上是可以**独立开关**的两层：核心代理转发+规则引擎永远跑在最省资源的路径上，而"记录到列表给用户看"是可选的重量级开销（涉及跨进程/跨线程通信、状态同步、UI 渲染、可能的持久化）。这对我们的架构有直接指导意义：3.12 节已经强调"统一协议下的批量聚合 + body 惰性拉取"来降低前端同步开销，在此基础上应更进一步——**在 Rust 后端设一个显式开关，控制"是否要把 Flow 元数据发送到前端/写入 SQLite"，而不是假设这一步总是发生**。这对"只是想临时开代理让流量能过、但不关心记录"的场景（比如只是为了让某个 App 的流量绕过某些网络限制）能大幅降低资源占用，列为 MVP 后的第一批增强功能。

### 0.1.3 移动端协作模式——第三方验证了系统代理按进程过滤的局限

官方 Collaboration 文档详细描述了"手机装 Reqable App + 桌面装 Reqable App 协同抓包"的完整流程，与 3.10.1 节的技术判断**逐点吻合**，是极有价值的第三方验证：

1. **问题陈述一致**：文档开篇列举了传统"手机设置 WiFi 代理转发到 Charles/Fiddler"方案的四个缺点——手动配置且用完要改回去、部分框架（**文档点名 Flutter**）不遵守系统代理设置、手动装根证书麻烦、WiFi 代理是全局的无法按 App 生效。**最后一点正是"系统代理无法按进程过滤"这个局限的用户可感知后果**（见 3.10.1 节）。
2. **配对机制**：桌面生成二维码，手机扫码配对（同局域网可自动发现直连），配对时**自动把桌面的根 CA 证书同步到手机**（但"安装到系统信任区"这一步说明白了**做不到自动化**，仍需用户手动走系统安装引导——这与我们 3.11 节"CA 安装无统一 API，只能引导式向导"的结论完全一致，Reqable 作为成熟商业产品也没有绕开这个平台限制）。
3. **流量转发机制**：明确写"**手机 App 启动本地 VPN service，把流量转发到桌面 Reqable，这正是它能够不需要 WiFi 代理就抓包的原因**"——这就是我们 3.10.1/3.10.2 节分析的"TUN/VPN 模式换取免配置接入能力，但要接受额外开销"的真实产品实现，且证实了移动端场景下 TUN/VPN 模式相比系统代理的**必要性而非可选性**（手机端没有"系统代理"这个显式配置概念对部分框架生效，VPN 模式是唯一能保证抓全的手段）。
4. **决定性的一句原话，直接印证 3.10.1 节的结论**：> "Reqable can detect application information on Android, but iOS does not support this due to technical limitations." **这与"Android 可以用 `ConnectivityManager.getConnectionOwnerUid` 做进程归因，iOS 沙箱下几乎不可能"的判断逐字吻合**——甚至连"iOS 是技术限制而非产品选择"这个措辞都完全一致。同时文档还提到 **"On Android, you can also capture traffic for specific apps and ignore others"**，即 Android 端不仅能做归因展示，还能做到**按 App 精确选择要不要抓**（对应 3.10.1 节"手段二"里 `VpnService.addAllowedApplication` 的能力）。
5. **Standalone 模式的额外发现**：文档提到如果手机不用协作模式而是独立本地抓包（"Standalone mode"），**Android/iOS 系统同一时刻只能激活一个 VPN**，如果用户同时需要科学上网类工具（走 VPN/tun2socks 实现）和 Reqable 抓包，两者会冲突——这是我们规划移动端配套 App 时必须提前告知用户的产品限制，不是我们实现能解决的问题。

**结论**：基于操作系统 API 特性推导出的"Android 能做、iOS 不能做、VPN 模式是免配置代价"这套结论，与市面最直接的商业竞品实际实现**完全对齐**，可以作为 `spec.md`/`plan.md` 移动端功能规划的可靠依据。

### 0.1.4 关键技术决策差异：Reqable 用真 Python3 做脚本引擎，而非嵌入式沙箱

这是与我们当前技术选型**明确不同**、需要重新权衡的一点。Reqable 的 Script 功能：
- `onRequest(context, request)` / `onResponse(context, response)` 两个入口函数，**依赖用户本机安装的真实 Python3 环境**（要求 3.6+，可手动指定 Python Home 路径），**可以直接 `import requests` 等任意第三方包**。
- 明确警告 `onRequest`/`onResponse` 运行在**不同进程**中，两者不能直接共享外部变量，须通过 `context.shared` 显式传递——说明 Reqable 的脚本执行是**每次调用都可能跨进程**（或者至少是与主进程隔离的独立解释器进程），而不是常驻的语言运行时嵌入。
- 明确"为防止滥用，移动端不提供该功能"——与 Gateway/Rewrite/Breakpoint 等规则类能力一样，全部是**桌面独占**的开发者功能。

**与我们计划中 `rquickjs`（QuickJS 嵌入式沙箱）方案的对比权衡**：

| 维度 | Reqable：真 Python3 子进程 | 我们计划：`rquickjs` 嵌入式 JS |
|---|---|---|
| 生态能力 | 可用完整 PyPI 生态（`requests`/`numpy`/加解密库等），对做过复杂 Mock/签名算法复现的重度用户是刚需 | 仅 JS 标准能力 + 我们自己注入的 API，无法 `npm install` 第三方包 |
| 环境依赖 | **依赖用户机器预装 Python3**，版本管理是用户负担，Reqable 甚至专门做了"指定 Python Home"的设置项来处理多版本共存问题 | 完全自包含在我们的二进制里，用户零配置，跨平台行为一致 |
| 性能/开销 | 每次调用可能有进程间通信/解释器启动开销（文档"不同进程"的说明印证了这点），不适合超高频路径 | 嵌入式沙箱同进程调用，理论上更快，适合我们已经规划的高吞吐 Flow 处理路径 |
| 安全隔离 | Python 子进程能访问用户完整文件系统/网络，隔离性明显弱于沙箱化的 QuickJS，Reqable 选择用"移动端直接禁用"来控制风险面，桌面端则完全信任用户自己的脚本 | QuickJS 天然沙箱化，可以做更细粒度的能力开放（例如只暴露我们定义的 API，不给文件系统访问） |
| 语言学习成本 | Python 对做测试/爬虫/安全的用户群体亲和度更高（这正是官网"development, testing, networking, security, web scraping"的目标用户画像） | JS/TS 对前端背景用户更友好，但目标用户群体（抓包/测试/安全工程师）不一定是前端背景 |

**本项目建议**：不必照搬 Reqable 的选择，但要吸取其背后的产品洞察——**目标用户群体（抓包调试/安全测试工程师）对 Python 生态的路径依赖是真实存在的**（`requests`、各种签名/加解密算法复现几乎是这个领域的通用语言）。折中方案：
1. **MVP 阶段维持 `rquickjs` 方案不变**（同进程、零依赖、沙箱安全的工程优势更契合我们"开箱即用"的定位，尤其是我们还要支持移动端场景，嵌入式方案的可移植性明显更好），
2. **中期可以补充一个可选的"外部 Python3 解释器"模式**作为进阶选项（类似 Reqable 的定位：仅桌面端、需用户自备环境、用于处理 QuickJS 沙箱做不到的复杂第三方库场景），实现上可以参考 Bruno 的"可切换沙箱强度"设计思路（1.2 节），做成 per-rule 可选择运行时，而不是替换掉默认方案。
3. 这个决策不阻塞 MVP，但应在 `spec.md` 的规则引擎设计里预留"脚本运行时可插拔"的抽象接口，为未来加 Python 运行时留好扩展点。

### 0.1.5 商业模式与账号体系的参考价值

Reqable 采用**免费桌面基础功能 + 云同步/团队协作等增值能力订阅制**的模式（`Cloud Sync Across Devices` 被列在 "More Features" 分类而非核心两大模块），API Collection 数据在未登录时"仅本地离线存储，仅当前设备可访问"，登录后才可云同步多端——这与 Yaak（1.1 节）的开源+云同步付费模式、Bruno 的纯本地文件+可选 Git 同步模式都不同，是**第三种商业模式路径**。虽然商业模式不是本文档技术调研的重点，但这个信息对我们决定"是否需要账号体系/云同步"这个产品决策点有参考价值——**至少证明"本地优先、账号可选"是这个品类里跑得通的模式**，不强制要求账号体系才能让核心功能可用。

---

## 1. API 客户端类产品调研

> **重要说明**：以下 Yaak/Bruno/Hoppscotch 三个项目均为**架构与实现参考对象**，用于回答"具体功能该怎么用 Rust/Tauri 实现"，它们并非本项目对标的产品形态——**Reqable（见上文 0.1 节）才是产品定位、功能范围层面真正对标的直接竞品**。Yaak 因为技术栈（Tauri+Rust+React）与我们完全一致，是最值得参考的**工程实现范例**，但不代表我们的产品要做成"Yaak 的样子"。

### 1.1 Yaak（`mountain-loop/yaak`）—— 最重要的工程实现参考对象

Yaak 本身就是 **Tauri 2.x + Rust + React 19**，与本项目技术栈完全一致，是最直接可借鉴的范例。MIT 协议。

**Cargo workspace 分层设计**（这是最值得借鉴的一点）：

```
crates/                 # 与 Tauri 完全解耦的"核心"逻辑
  yaak-core, yaak-common, yaak-crypto, yaak-git
  yaak-http, yaak-grpc, yaak-ws, yaak-sse   # 各协议客户端
  yaak-models                                # SQLite 数据模型
  yaak-plugins                               # 插件管理（Node 子进程 RPC）
  yaak-templates                             # 模板变量渲染引擎
  yaak-sync                                  # 文件系统同步（可选）
  yaak-tls, yaak-api, yaak-proxy

crates-tauri/           # Tauri 专属胶水代码
  yaak-app-client        # 实际的桌面 App crate（tauri::command 入口）
  yaak-window, yaak-mac-window, yaak-system-appearance
  yaak-fonts, yaak-license, yaak-tauri-utils

crates-cli/yaak-cli      # 独立 CLI 二进制，复用同一份 SQLite 数据库
crates-proxy/            # 独立的小型代理 Tauri App，复用核心 crate
```

**关键设计原则**：核心业务逻辑（发请求、存数据、渲染模板）完全不依赖 `tauri::AppHandle`，只在 `crates-tauri/` 里做薄薄一层 `#[tauri::command]` 包装。这带来的好处：
- 可以零成本复用出一个 headless CLI；
- 未来做"代理内核"和"UI 客户端"两个 Tauri App 共享同一套核心时也不用重写；
- 单元测试可以完全绕开 Tauri runtime。

**HTTP 执行**：全部在 Rust 侧用 `reqwest`（而非 WebView 的 `fetch`），原因是要绕开浏览器 CORS/Cookie 限制、需要完整控制 TLS/重定向/超时/流式传输。同时支持 rustls 和 native-tls 双 TLS 后端（后者用于兼容企业自签名/MITM 代理场景），支持 SOCKS 代理和流式 body。

**多协议支持**：
- gRPC：`tonic` + `prost` + `prost-reflect`（支持 Server Reflection，无需预编译 `.proto`）
- WebSocket：独立 `yaak-ws` crate（`tokio-tungstenite`）
- SSE：独立 `yaak-sse` crate（`eventsource-client`）

**数据存储**：**SQLite**（`rusqlite` + `r2d2` 连接池 + WAL 模式），用 `sea-query` 做类型安全查询构建（不用重量级 ORM）。大 body/附件放在单独的 SQLite 文件里隔离。文件系统同步（YAML 导出 + `notify` 监听）是可选的第二层，不是主存储——这与 Bruno 的"文件即数据库"理念相反。

**类型共享的杀手锏**：Rust struct 上打 `#[ts(export)]`（`ts-rs` 库）自动生成 TypeScript 类型定义文件，Rust 端改了字段前端类型自动同步，避免手写两份 schema 出现漂移（这是 Bruno 明确踩过的坑，见下文）。

**前端**：React 19 + **Jotai**（原子化状态，而非 Redux）+ TanStack Query/Router/Virtual + CodeMirror 6。

**插件系统**：插件是真正独立的 **Node.js 子进程**，通过本地 WebSocket 与 Rust 主进程做 RPC 通信（用 `yaak-rpc` crate 定义的 `ts-rs` 共享类型）。这样插件能用完整的 Node 生态（真 `fetch`、npm 包），同时 Rust 主进程和插件之间的边界很窄、类型安全。插件分类是强类型的"扩展点"而非任意脚本：`auth-*`、`importer-*`、`filter-*`、`template-function-*`、`action-*`。

### 1.2 Bruno（`usebruno/bruno`）—— Electron 路线，文件优先理念

MIT 协议，Electron + React。核心亮点是它的**文件即数据（file-first）**理念——这是与 Yaak 完全相反的哲学，直接源于"让 Collection 可以被 Git diff/合并"这一诉求。

**`.bru` 格式**（用 ohm-js PEG 语法解析）：

```
meta {
  name: Get Users
  type: http
  seq: 1
}

get {
  url: {{baseUrl}}/users
  body: none
}

headers {
  Authorization: Bearer {{token}}
}

script:pre-request {
  bru.setVar("startTime", Date.now());
}

tests {
  test("status is 200", function() {
    expect(res.getStatus()).to.equal(200);
  });
}
```

设计约束（来自 Bruno 自己的架构文档）：纯文本、逐行可 diff、一个请求一个文件、解析器与序列化器必须做到 `parse(stringify(x)) === x` 无损往返、新字段必须是可选的以保证向后兼容。这套约束对我们设计自己的存储格式也有参考价值——**即便我们像 Yaak 一样以 SQLite 为主存储，也应该提供一个"导出为纯文本可 Git 管理格式"的能力**，因为这是 API 协作工具的一个高频真实需求。

**脚本沙箱**：Bruno 提供两种可选运行时（按 Collection 配置）：
- **QuickJS**（WASM 沙箱，安全模式，默认）——真隔离，Node API 不泄漏进去；
- **Node VM**（`node:vm`，开发者模式）——可以用 npm 包，但隔离性较弱。

这个"可切换沙箱强度"的设计思路值得借鉴：如果我们要支持 Postman 兼容的 pre-request/test 脚本，可以用 **`rquickjs`**（QuickJS 的 Rust 绑定）在 Rust 侧原生跑 JS 沙箱，不必像 Bruno 一样再起一个 Node 进程。

**前端**：React 19 + Redux Toolkit v1（注意锁定在 v1 不是 v2）+ styled-components v5 + CodeMirror 5（旧版）。

### 1.3 Hoppscotch —— Web 优先 + 三代 Interceptor 演进

Vue 3 + Vite + pnpm workspace，`hoppscotch-common` 是与平台无关的共享 UI 包，`hoppscotch-desktop`（Tauri）和 `hoppscotch-selfhost-web` 都是薄壳。

最值得借鉴的是它的 **Interceptor（网络执行层）演进**，因为这正是我们要解决的"Web 沙箱 vs 原生网络访问"问题的三种真实方案：

1. **浏览器扩展**：content script 桥接，绕开页面的 CORS 限制——只对纯 Web 版本有意义。
2. **Proxy 转发**（"Proxyscotch"）：一个 Go 编写的中转代理服务器。
3. **Hoppscotch Agent**：独立的 Tauri v2 小应用，常驻后台监听 `localhost:9119`，用一次性 6 位数 OTP 配对，之后用 **X25519 密钥交换 + AES-256-GCM** 加密所有请求/响应。实际发请求用 Rust `hoppscotch-relay` crate（自研的 curl-rs fork，支持客户端证书、自定义 CA、HTTP 代理+NTLM、按域名跳过证书校验）。
4. **`hoppscotch-desktop`**：直接把 `hoppscotch-relay` 当 Tauri command 调，跳过本地回环通信——**这正是我们应该采用的模式**，因为我们从第一天就是原生桌面应用，不需要 Hoppscotch 那种"Web App 需要一个本地伴生进程"的折中方案。

**`hoppscotch-kernel` 抽象层**：定义了 `RelayV1`（`execute(request) -> {cancel, emitter, response}`）、`IoV1`（文件对话框/外部链接）、`StoreV1`（加密压缩可监听的 KV 存储）等接口，让 UI 层完全不关心底层是浏览器扩展还是原生 Tauri 调用在真正执行网络请求。**这个"能力接口抽象"模式值得直接照搬**：我们可以定义 `TrafficCapture`、`RequestExecutor`、`CertStore` 等接口，UI 侧统一走 Tauri command，不关心背后是内嵌 MITM 代理还是普通 fetch。

**编辑器**：CodeMirror 6 为主（body 编辑、脚本编辑、自研 `codemirror-lang-graphql` 包），Monaco 作为个别高阶场景的补充依赖（如 diff 视图）——CM6 主力、Monaco 点缀的组合是三个项目里的主流做法。

### 1.4 三个 API 客户端项目对比表

| 维度 | Yaak | Bruno | Hoppscotch |
|---|---|---|---|
| 壳 | Tauri 2.x | Electron | Web / Tauri（薄壳）|
| HTTP 引擎 | Rust reqwest/hyper | Node axios | Rust curl-rs fork（Agent/Desktop）|
| gRPC | tonic + prost-reflect | @grpc/grpc-js | - |
| 数据存储 | **SQLite**（rusqlite+r2d2+sea-query）| **纯文本文件**（.bru/.yml）| 云端同步 + 本地 IndexedDB |
| 类型共享 | Rust→TS 自动生成（ts-rs）| 手写两份（Yup schema + TS type）|- |
| 脚本沙箱 | 无任意脚本，强类型插件+Node子进程RPC | QuickJS(WASM) 或 Node VM 可选 | QuickJS/Web Worker |
| 前端框架 | React 19 + Jotai + TanStack | React 19 + Redux Toolkit v1 | Vue 3 + 自研 DI(`dioc`) |
| 编辑器 | CodeMirror 6 | CodeMirror 5（旧）| CodeMirror 6 主 + Monaco 点缀 |

---

## 2. MITM 抓包代理类产品调研

### 2.1 mitmproxy（Python）—— 协议无关 Flow 状态机架构

**TLS 拦截机制**（`mitmproxy/certs.py` + `addons/tlsconfig.py`）：
- 用 `cryptography` 库生成 RSA 根 CA（`BasicConstraints(ca=True)`，10 年有效期）。
- 每遇到新域名，现场签发叶子证书（`dummy_cert`），写入 SAN，显式设置 `AuthorityKeyIdentifier`（避免部分严格校验器报错），叶子证书 199 天有效期，`not_valid_before` 回拨 2 天容忍时钟误差。
- 用 `CertStore` 做 LRU 缓存（容量 100），避免重复签发。
- **SNI 嗅探**在 TLS record 层手工解析 ClientHello（`parse_client_hello`），拿到域名后再决定要不要"先连真实服务器再回应客户端"（`eager`/`lazy` 两种策略）。
- 底层用 **pyOpenSSL**（而非标准库 `ssl`）以获得细粒度 BIO 控制和自定义 ALPN 协商回调。

**协议分层架构**（最值得借鉴的架构思想）：mitmproxy 用"洋葱式 Layer 组合"（modes layer → TLS layer → HTTP layer → WebSocket layer），且把 HTTP/1(h11)、HTTP/2(hyper-h2)、HTTP/3(aioquic) 三种协议的具体报文格式差异，全部收敛成统一的内部事件（`RequestHeaders`/`RequestData`/`RequestEndOfMessage`/...），再驱动同一个协议无关的 `HttpStream` 状态机去触发 addon 钩子。即：**协议无关的 Flow 状态机 + 协议特定的编解码层**这个抽象，是我们在 Rust 里应该效仿的核心设计。

一个反直觉但重要的细节：mitmproxy 在显式代理模式下，**客户端到代理这一段强制用 HTTP/1.1**（`client_alpn = b"http/1.1"`），即使客户端支持 h2 也会降级——只有代理到真实服务器这一段才可能用 h2。这是为了降低实现复杂度做出的权衡，我们自己实现时也可以采用同样的简化。

**WebSocket 拦截**：基于 `wsproto` 库，响应 `101 Switching Protocols` 时切换到 WS 层，两端各维护一个 `wsproto.Connection`，逐帧转发，支持编辑/丢弃帧、处理 `permessage-deflate` 压缩扩展。

**Addon（插件）架构**：基于**方法名反射**（duck typing）——任意对象只要定义了 `request(flow)`/`response(flow)`/`websocket_message(flow)` 等同名方法就会被自动发现调用，支持同步/异步混用，可通过抛 `AddonHalt` 短路后续 addon。这套 Python 风格的设计在 Rust 里不适用，更适合用 **trait 对象 + 显式方法**（`fn handle_request(&self, req) -> RequestAction` 这类签名），我们自己的 `RuleEngine`/`InterceptHandler` 采用同样的思路实现，具体设计见 `spec.md` 第 4 节。

**代理模式**：`HttpProxy`（显式代理，解析 CONNECT）、`TransparentProxy`（依赖操作系统 iptables/pf 重定向，代理通过 `SO_ORIGINAL_DST` 拿真实目标地址）、`ReverseProxy`、`Socks5Proxy`（完整手写状态机）。

**Flow 数据模型**：`Flow` 基类含 `client_conn`/`server_conn`/`intercepted`（配合 `asyncio.Event` 实现暂停等待用户放行）/`marked`/`comment`/`is_replay`/`killable`，`HTTPFlow` 组合 `Request`+`Response`。**关键设计**：Flow 必须能整体序列化/反序列化（`get_state()`/`set_state()`），且与 UI 展示层解耦——这是做"可持久化、可回放"调试代理的核心要求，我们也要照做。

**证书锁定 (cert pinning) 应对**：mitmproxy 本身不做绕过，只提供"让浏览器信任自己的 CA"；真正绕过 pinning 需要外部工具（Frida hook）动态 patch 客户端校验逻辑，这不在 mitmproxy/我们的核心范围内，但可以作为进阶功能考虑（参考 HTTP Toolkit 的 Frida 集成）。

### 2.2 HTTP Toolkit（mockttp + httptoolkit-server）—— 系统集成范例

这一节内容基于对 `httptoolkit/mockttp` 仓库 `src/server/http-combo-server.ts` 与 `src/util/certificates.ts` 的实际源码逐行阅读（而非仅 README 概述），补充记录以下具体实现细节。

**证书生成（`certificates.ts`）深入细节**：
- CA 证书用 WebCrypto 的 `crypto.subtle.generateKey` 生成 RSA-2048，**扩展字段按 Baseline Requirements 规定的固定顺序排列**（`countryName`→`organizationName`→`organizationalUnitName`→`commonName`），这个细节很容易被忽略但实际影响部分严格客户端的证书解析兼容性。
- `notBefore` 故意回拨 24 小时（容忍客户端时钟误差），这个细节在 investigation.md 其他章节提到过类似思路（mitmproxy 同样回拨 2 天），是业界通用工程实践，我们自研的 `cuckoo-ca` 也应该同样处理。
- **Name Constraints 的具体实现**：直接用 `@peculiar/asn1-x509` 手写 `NameConstraints`/`GeneralSubtree`/`GeneralName` ASN.1 结构并 `AsnConvert.serialize()` 序列化后作为自定义 `x509.Extension` 插入证书，因为 `@peculiar/x509` 高层 API 本身不直接支持这个扩展字段。这提醒我们：用 `rcgen` 时如果需要类似的非标准/高级扩展字段，也很可能需要直接操作底层 `x509-cert`/`der` crate 而不是 `rcgen` 高层 `CertificateParams`。
- **证书缓存**（`http-combo-server.ts` 中的 `getSecureContext`）：用 `Map<string, { context, expiresAt }>` 缓存 `tls.SecureContext`，命中判断条件是 `expiresAt - now > 1小时缓冲`，提前 1 小时失效而非到期才失效——避免证书刚好在握手过程中过期的竞态条件，这个缓存策略细节值得我们 `DashMap` 缓存设计时借鉴。
- **Certificate Transparency SCT 伪造的实际机制**（`certificate-transparency.ts` + `embedSCTsAndSign`）：思路比想象中更巧妙——**从 CA 证书的公钥确定性地派生出两个虚拟 CT log operator**（`deriveCTLogOperators(caCert)`），先用一个占位签名（`RSA_PLACEHOLDER_SIGNATURE`/`EC_PLACEHOLDER_SIGNATURES`，字节长度与真实签名算法对齐但内容全 0）构造一个临时证书骨架来计算 SCT 所需的待签名数据，再把计算出的 SCT 嵌入扩展字段后用真实 CA 私钥重新对整个证书签名一次。这是一个相对小众但技术含量很高的反检测手段，**建议列为我们 v2+ 的高级/进阶功能**，不阻塞 MVP。
- **JA3/JA4 指纹计算**：`http-combo-server.ts` 在 `analyzeAndMaybePassThroughTls` 里对每个进来的 TLS 连接都会计算 `calculateJa3`/`calculateJa4`（基于解析出的 ClientHello 的加密套件顺序/扩展列表等）并挂在 socket 元数据上。JA3/JA4 是业界标准的 TLS 客户端指纹算法，可以作为我们产品的**进阶功能**：在 Flow 列表里展示每个请求的 JA3/JA4 指纹，帮助用户区分真实浏览器 vs 自动化脚本/App SDK 流量。需要在 `sniff.rs`/`tls.rs` 里把解析出的 ClientHello 原始字节保留下来才能计算（`rustls` 的 `ClientHello` API 默认不暴露原始字节，可能需要在 `LazyConfigAcceptor` 拿到原始 ClientHello record 后手动解析，或引入专门的 ClientHello 解析小库）。

**"Combo Server" 设计（`http-combo-server.ts`）深入细节**：单个端口同时支持 HTTP/HTTPS/HTTP2/SOCKS/未知协议，实际上是直接调用 `httpolyglot.createServer({ tls, socks, unknownProtocol, http, http2 }, requestListener)` 把分流逻辑整个交给这个库，HTTP Toolkit 自己没有手写 peek 判断代码。几个值得借鉴的具体细节：
- **ALPN 协商用动态回调而非静态列表**（Node 20.4+）：`ALPNCallback: ({ protocols }) => 优先协议 或 clientProtocols[0]`——优先选我们想要的协议，但客户端只提供不认识的协议时也不拒绝握手（直接接受它的第一个），比静态 `ALPNProtocols` 列表更宽容，避免因为不认识的 ALPN 值直接握手失败。`rustls` 的 `ServerConfig` 同样支持自定义 ALPN 选择逻辑，我们实现时应采用类似的容错策略。
- **CONNECT 处理极简**（`handleH1Connect`）：直接写回 `200 OK` 后 `server.emit('connection', socket)` 把 socket 重新丢回自身触发协议再识别，复用同一套 combo server 分流逻辑处理隧道内部的流量，而不是另外写一套隧道内处理代码。这与 2.5 节 hudsucker 分析中提炼的"tunnel-in-tunnel 递归"思路完全一致，仅调用形式不同（Node 的 EventEmitter 循环 vs Rust 的显式递归函数调用）。
- **HTTP/2 CONNECT 支持**（`handleH2Connect`）：识别 `:authority` 伪头部作为隧道目标地址——用 HTTP/2 本身作为代理层协议（客户端到代理这一跳走 h2 CONNECT，RFC 8441/Extended CONNECT），而不仅仅是隧道内部承载的内容协议可能是 h2。现代浏览器/客户端的正向代理场景理论上可能用上 h2 CONNECT，`connect.rs` 设计时需预留这个分支（即使 MVP 阶段只处理 HTTP/1.1 CONNECT，也要在协议探测层面考虑到 h2 帧开头的 CONNECT 请求）。
- **socket 元数据继承链**（`inheritSocketDetails`）：CONNECT 隧道内层新建的 TLS socket/HTTP2 stream 需要从外层 socket 继承 `localAddress`/`remoteAddress`/计时信息/`SocketMetadata` 等，用 `Object.defineProperties` 强制这些字段可写（因为 HTTP/2 stream 对象默认会阻止外部写入这些属性）。这是一个容易被忽略但影响"多层隧道场景下 Flow 元数据是否完整"的实现细节，我们的 `FlowContext` 在 tunnel-in-tunnel 递归时也需要设计类似的元数据传递机制。
- **TLS 握手失败诊断**（`ifTlsDropped` + monkey-patch `tls.TLSSocket.prototype._init`）：HTTP Toolkit 通过 monkey-patch Node 内部 TLS 实现的 `_init` 方法，在 SNI 回调（`oncertcb`）触发的**握手尚未完成阶段**就提前把 `servername`/`remoteAddress` 挂到 socket 上，这样即使握手最终失败（比如客户端因证书不信任而中断），也能拿到诊断信息用于展示"这个域名的 HTTPS 拦截失败了"。此外还有一套**基于时间窗口的启发式算法**判断"客户端是否拒绝了我们的证书"：TLS 握手成功后等待一段时间（`Math.max(tls握手耗时 * 10, 100ms)` 作为超时阈值）看客户端是否发送了任何数据，如果在这个窗口内静默关闭连接且从未发送数据，就推断为"客户端拒绝了证书"（因为很多客户端在证书校验失败时不会发送标准 TLS alert，而是直接静默断开）。这套诊断机制对我们做"证书安装向导"页面的错误提示非常有参考价值——**建议在 `cuckoo-ca`/`tls.rs` 模块里实现类似的握手失败归因逻辑**，能大幅提升用户排查"为什么这个 App 的流量抓不到"问题时的体验。

**TLS Passthrough 深入细节**（`analyzeAndMaybePassThroughTls` 函数）：实现方式是**先移除 TLS server 默认的 `connection` 监听器，替换成自己的逻辑**——读取 `readTlsClientHello(socket)` 解析出 SNI/ALPN/JA3/JA4 后，如果命中 passthrough 域名规则，直接调用透传回调（不再继续 TLS 握手，原始加密字节双向转发）；否则手动调用回原始的 TLS connection listener 继续正常握手流程。这种"劫持默认 listener、按条件决定是否继续走标准流程"的模式，比"提前判断再决定要不要建 TLS server"这种思路更简洁——我们在 Rust 里用 `LazyConfigAcceptor` 天然就有这个能力（读完 ClientHello 后可以选择 `start_handshake()` 还是直接转发原始 socket），不需要专门"劫持监听器"这种 hack，反而是 Rust 方案的一个先天优势。

**客户端接入方案**（`httptoolkit-server/src/interceptors/`）——这是本项目"如何让流量真正进入我们的代理"这一问题的最全面参考：
- **系统代理/浏览器**：启动全新浏览器实例并指定 `--proxy-server`，独立用户数据目录预置根证书信任。
- **终端环境变量注入**：`HTTP_PROXY`/`HTTPS_PROXY`/`NODE_EXTRA_CA_CERTS`/`SSL_CERT_FILE`/`JAVA_TOOL_OPTIONS` 等，覆盖 npm/pip/curl/Git/Cargo/Ruby/PHP 等几乎任何语言进程。
- **Android/ADB**：`adbkit` 连接、root 检测、`tmpfs` 挂载覆盖 `/system/etc/security/cacerts`、Android 14+ APEX Conscrypt 目录逐进程 mount namespace 注入、非 root 设备用 `adb reverse` + Network Security Config。
- **Docker**：Docker daemon API 代理改写 create/build 请求注入代理环境变量、自建 DNS 服务器解析容器名、SOCKS5 隧道代理访问容器网络。
- **Frida (iOS/Android)**：动态注入反 SSL Pinning 脚本。

**架构分工**：`httptoolkit-ui`（纯前端 SPA，可跑浏览器）与 `httptoolkit-server`（本地常驻 Node 进程，跑 mockttp 代理内核 + 特权系统操作，暴露 REST/GraphQL API 给 UI）分离。**这个分工模式几乎就是 Tauri 的天然形态**：Rust 后端 = "httptoolkit-server"（代理内核+系统集成），WebView 前端 = "httptoolkit-ui"（纯展示），两者通过 Tauri IPC 通信，不需要 HTTP Toolkit 那种额外的本地 REST/GraphQL 层。

### 2.3 mitmproxy_rs —— 不能直接复用，但架构值得参考

这一节基于对 `mitmproxy/mitmproxy_rs` 仓库实际源码（`mitmproxy-rs/src/lib.rs`、`stream.rs`、`server/wireguard.rs`、顶层 `Cargo.toml`）的直接阅读，而非仅凭 README 概述。

`mitmproxy/mitmproxy_rs` 是给 mitmproxy Python 主程序用的 **PyO3 绑定 crate**，专门解决"如何把任意设备/进程的流量导到 mitmproxy 监听端口"这一个问题。它是一个 **Cargo workspace**，成员包括：
- `mitmproxy-macos/`（含 `certificate-truster` 子 crate）：macOS Network Extension（系统扩展，需签名公证）
- `mitmproxy-windows/`（含 `redirector` 子 crate）：基于 WinDivert 的流量重定向
- `mitmproxy-linux/` + `mitmproxy-linux-ebpf/` + `mitmproxy-linux-ebpf-common/`：基于 **Aya**（纯 Rust eBPF 框架，`aya`/`aya-ebpf`/`aya-log`/`aya-log-ebpf` 均在 workspace 依赖里显式声明）实现的用户态-内核态分离 eBPF 程序（`-ebpf` 后缀的 crate 是真正跑在内核里的 eBPF 字节码，`-ebpf-common` 是用户态和内核态共享的数据结构定义）
- `mitmproxy-contentviews/` + `mitmproxy-highlight/`：**协议内容查看器/语法高亮也用 Rust 实现**——`lib.rs` 里注册了 `HexDump`/`HexStream`/`MsgPack`/`Protobuf`/`GRPC` 这些 `Contentview`，说明 mitmproxy 把"格式化展示 gRPC/Protobuf/MessagePack 等结构化二进制内容"这类原本看起来是纯 UI 层面的工作，也下沉到了 Rust 侧实现（大概率是为了性能和代码复用，Python UI 和未来可能的 Web UI 都能调用同一份解析逻辑）。这对我们有直接参考价值：**content view/格式化展示逻辑也可以放进 Rust 后端而不是前端 JS**，尤其是 Protobuf（需要 `.proto` 反射）这类需要复杂解析逻辑的场景。
- 顶层 `src/`（crate 名 `mitmproxy`）：核心协议栈，`Cargo.toml` 显式依赖 **`smoltcp`**（纯 Rust 用户态 TCP/IP 协议栈）+ **`GotaTun`**（Cloudflare 出品的纯 Rust WireGuard 实现）+ `internet-packet`（mitmproxy 团队自己写的 IP 包解析 crate，启用了 `smoltcp` feature）+ `hickory-resolver`（纯 Rust DNS 解析）+ `tun`（跨平台 TUN 设备封装）。这证实了 WireGuard 模式的技术栈确实是 **`GotaTun` 处理 WireGuard 协议本身，`smoltcp` 在收到解密后的 IP 包后跑一个极简用户态 TCP/IP 协议栈来生成 TCP/UDP 层的 `Stream` 抽象**，而不是简单地把包转发到系统网络栈。

**`Stream` 抽象是一个教科书级的"命令模式 + 跨语言异步桥接"实现**（`mitmproxy-rs/src/stream.rs`）：Rust 侧 `Stream` struct 只持有一个 `command_tx: mpsc::UnboundedSender<TransportCommand>`，`read()`/`write()`/`drain()`/`close()` 等方法全部是"构造一个 `TransportCommand` 变体（`ReadData`/`WriteData`/`DrainWriter`/`CloseConnection`）发送到 channel，然后返回一个 Python awaitable"，真正的 IO 逻辑在别处（channel 的接收端）统一处理，`Stream` 本身不直接碰 socket。跨语言异步桥接用的是 `pyo3_async_runtimes::tokio::future_into_py()`，把 Rust `Future` 包装成 Python 侧的 `awaitable`，这样 Python 的 `asyncio` 代码可以直接 `await stream.read(n)`，底层其实是在等一个 `tokio::sync::oneshot::Receiver`。这个模式对我们没有直接复用价值（我们不需要跨语言桥接），但**"用命令枚举 + channel 解耦上层 API 语义和底层 IO 执行"这个设计思想本身值得参考**，尤其是如果未来我们的代理内核想支持"暂停单个连接的读取但不阻塞其他连接"这类精细控制时，可以复用类似的命令消息模式，而不是直接在 `read()` 调用栈里同步操作 socket。

**结论：不能直接作为通用库使用**（与 PyO3/Python GIL 强耦合，平台特定二进制部分的构建脚本较复杂）。但其思路（WireGuard 模式实现"手机免配置抓包"）和具体依赖选型（`GotaTun` + `smoltcp` + `aya`）值得作为**远期**方向的直接参考——如果我们后续要做同类能力，`Cargo.toml` 依赖清单几乎可以照抄。MVP 阶段用系统代理设置已经足够覆盖大部分场景。

**补充发现（读 `mitmproxy/proxy/server.py` 最新源码得到）**：`mitmproxy_rs` 的用途已经不止"流量导流"，mitmproxy 主进程的 `ConnectionIO` 现在把 `reader`/`writer` 类型定义为 `asyncio.StreamReader | mitmproxy_rs.Stream`，即**部分连接的底层读写已经切换成 Rust 实现的 `Stream`**（尤其是 UDP/WireGuard 场景，`mitmproxy_rs.udp.open_udp_connection` 直接返回 Rust 侧对象）。也就是说，连 mitmproxy 这样一个以"Python 实现"著称的老牌项目，也在把性能敏感、字节处理密集的底层网络栈逐步迁移到 Rust，Python 侧只保留 `layer.py`/`commands.py`/`events.py` 这套协议无关的事件驱动编排逻辑。这从另一个角度印证了"底层协议字节处理适合用 Rust 直接实现"这一判断——我们直接用 Rust 写整个内核，反而省去了 mitmproxy 那种跨语言绑定（PyO3 FFI 边界、GIL 交互）的额外复杂度。

mitmproxy 的调度模型也值得记录：`ConnectionHandler` 是一个**纯事件驱动、无 IO 副作用的状态机 + 外层 asyncio 循环负责真正执行 IO** 的两层架构——`Layer.handle_event(event) -> list[Command]`，`server_event()` 收到网络事件（`DataReceived`/`ConnectionClosed`）后 dispatch 给最外层 `NextLayer`，layer 内部逻辑返回 `OpenConnection`/`SendData`/`CloseConnection` 等**命令对象**（而不是直接 await 网络调用），再由 `ConnectionHandler` 统一执行。这种"状态机与 IO 分离"的设计可测试性极强（可以完全脱离真实网络单测 layer 的协议逻辑），是比我们当前 spec.md 里"直接在 async fn 里顺序 await"更解耦的架构，复杂度也更高——**建议列为代理内核的中期演进方向，不作为 MVP 阶段的架构要求**（当前 plan.md 已强调"先做能跑的最小版本，再逐步补齐"，与这个判断一致）。值得注意的是，`mitmproxy_rs.Stream` 的命令模式与 Python 侧 `Layer` 的命令模式（`OpenConnection`/`SendData`）在设计哲学上是一致的——**整个 mitmproxy 项目从 Python 核心到 Rust 扩展，统一使用"状态机产生命令、外层统一执行 IO"这一套架构范式**，这是它能够在混合语言、混合运行时的情况下依然保持架构一致性的关键。

### 2.4 Google Martian / elazarl-goproxy（Go）—— 与我们自研思路完全一致的直接参照

除了 mitmproxy（Python）和 HTTP Toolkit（Node），Go 生态里两个最有代表性的开源 MITM 库读下来会发现**做法与我们计划的自研方案几乎一模一样**，这进一步验证了"自研代理内核、只复用底层协议库"是行业通用做法，而非我们自己凭空发明的取舍：

**Google Martian**（`google/martian`，已归档但设计被很多商业代理沿用）的 `mitm/mitm.go`：证书缓存是自己手写的 `map[string]*tls.Certificate` + `sync.RWMutex`，命中时校验 `tlsc.Leaf.Verify()` 是否过期，未命中/过期则调用标准库 `x509.CreateCertificate()` 现场签发。TLS 握手完全交给 Go 标准库 `crypto/tls`（`GetCertificate` 回调在握手时被动态调用），Martian 自己不碰 TLS 握手过程本身。这与我们 spec.md 里 `DashMap<String, Arc<ServerConfig>>` 证书缓存 + `rustls` 负责握手的分工思路完全对应，只是语言从 Go 换成 Rust。

**elazarl/goproxy**（2.5k+ star，最流行的 Go MITM 库之一）的 `https.go` 是更贴近我们要实现内容的参照，关键设计：
- **1 字节 peek 判断 TLS**：`peek[0] == 0x16`（TLS handshake record 类型）即为 TLS 分支，否则走明文/WebSocket 分支——比我们计划 peek 前几个字节更激进，但思路一致。
- **自己写了 `internal/http1parser` 内部包**做 HTTP/1.1 报文解析，而不是用 Go 标准库 `net/http` 的高层 server——原因与我们 spec.md 里的判断一致：标准库会规范化/丢弃原始报文的字节级细节（header 顺序、大小写、畸形格式），MITM 场景需要保真转发。
- **HTTP/2 检测技巧值得借鉴**：不依赖 ALPN，而是在按 HTTP/1.1 解析请求时发现 `req.Method == "PRI"`（HTTP/2 client preface 固定以 `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n` 开头），识别后 `reader.Discard(6)` 丢弃剩余的 `SM\r\n\r\n` 前导字节，再切换到专门的 `H2Transport` 处理——这是一种"先尝试按 HTTP/1.1 解析，从首行识别出 h2 前导码再切换协议"的兜底方案，可以作为我们协议探测逻辑里 ALPN 分流之外的补充路径（应对客户端不走标准 ALPN 协商 h2 的场景，如某些 h2c 明文升级）。
- **WebSocket 识别**：检查响应是否为 `101 Switching Protocols` + `Upgrade` 头，命中后将 `resp.Body` 置空（避免标准库尝试按 HTTP body 语义读取），转交给 `proxyWebsocket()` 做双向裸字节转发。
- **CONNECT 隧道透传（非 MITM 场景）**：用 `halfClosable` 接口做双向 `io.Copy` + `CloseWrite()`/`CloseRead()` 半关闭语义，这个"一端读到 EOF 后半关闭对应方向，而不是立刻整体关闭连接"的细节，是保证 keep-alive/流式响应正确性的常见坑点，我们在 `forward.rs` 里也需要同样处理（Rust 对应 `tokio::io::copy_bidirectional` 或手写双向转发时用 `TcpStream::shutdown()` 的写方向）。

### 2.5 hudsucker —— 架构参考对象（明确不作为依赖使用）

**决策：不使用 `hudsucker`（或其他任何整体封装的 Rust MITM 代理库）作为依赖**，MITM 代理内核完全自主实现。理由：

1. **控制粒度**：整体封装库把"CONNECT 处理→协议探测→TLS 握手→HTTP 转发"这条链路封装成了一个黑盒 `Proxy::start()`，只在几个预留的 Handler 钩子（`handle_request`/`handle_response`/`should_intercept_tls`）上开洞。我们需要的很多能力（帧级 HTTP/2 可视化、自定义的断点暂停/恢复语义、灵活的规则引擎介入时机点、精确的分阶段计时打点、tunnel-in-tunnel 递归转发逻辑本身的可观测性）都需要深入链路内部，套一层第三方黑盒后再挖洞反而更麻烦。
2. **依赖健康度**：`hudsucker` 是一个 363 star 的小众项目，核心代理逻辑作为整个应用最关键的模块之一，不应绑定在一个维护活跃度存在不确定性的第三方 crate 上——一旦上游停更或引入 breaking change，我们的核心能力会被动受制于人。
3. **吃透协议细节，服务长期演进**：自己实现 CONNECT 隧道、协议探测、TLS 动态签发/终止、HTTP/1.1 与 HTTP/2 状态机、WebSocket 帧转发这一整条链路，能让团队真正吃透这些协议的字节级细节，这对后续扩展 HTTP/3 MITM、做帧级可视化、做更复杂的断点/重放语义是必要的知识基础；反之如果一开始就依赖黑盒库，后续想突破库的能力边界时会非常被动。

**但这不意味着从零手写 TLS 握手或 WS 帧解析**——那类底层字节级协议编解码没有自研的必要，且更容易引入安全漏洞（TLS 实现的正确性和抗攻击性极其重要）。因此技术边界划分为：

| 层次 | 是否自研 | 说明 |
|---|---|---|
| TCP 监听、accept 循环 | ✅ 自研 | 用 `tokio::net::TcpListener`，很薄的一层 |
| 协议探测（peek 首字节区分 TLS/明文 HTTP/WS） | ✅ 自研 | 参考 hudsucker/httpolyglot 的思路，逻辑简单，几十行代码 |
| CONNECT 隧道处理、tunnel-in-tunnel 递归转发 | ✅ 自研 | 核心代理行为逻辑，必须完全掌控 |
| TLS 握手/记录层编解码 | ❌ 用 `rustls` | 不重新实现 TLS 协议本身，只是**驱动** `rustls` 的 API（`LazyConfigAcceptor` 做 ClientHello 先读后握手） |
| 证书签发的密码学操作（RSA/ECDSA 签名） | ❌ 用 `rcgen` | 不重新实现 x509 编码和签名算法，只是**调用** `rcgen` 生成证书，签发策略（何时签、给谁签、有效期多久）由我们自己控制 |
| HTTP/1.1 报文解析 | ✅ 自研（或极薄层复用 `httparse`）| 需要保留原始 header 顺序/大小写/重复 key 以还原线路真实字节，这是通用 HTTP 库通常不保证的，值得自己写一个薄的状态机 |
| HTTP/2 帧解析 | ⚠️ 复用 `h2` crate 的底层帧类型，自己驱动状态机 | `h2` 提供帧级 API，我们自己组装成"抓包代理"需要的状态机和事件模型，而不是用 `hyper` 的高层封装（那样看不到帧） |
| WebSocket 帧编解码 | ❌ 用 `tokio-tungstenite` | 不重新实现 WS 帧格式（掩码/分片/opcode），只是**用它编解码**，帧转发的业务逻辑（是否拦截/修改/丢弃）自己写 |
| 拦截规则引擎、断点暂停/恢复、Flow 数据模型 | ✅ 完全自研 | 这是我们产品的核心差异化能力，必须自己设计 |

**从 hudsucker 源码中仍值得借鉴的具体设计点**（仅作为思路验证，不引入依赖）：
- **协议探测**：CONNECT 建立隧道后 peek 前几个字节，`b"\x16\x03"`（TLS record 头）走 TLS 分支，`b"GET "` 走明文/WS 分支，其他情况直接双向透传兜底——这个判断逻辑本身很简单，可以直接在我们自己的 accept 循环里写。
- **TLS 分支用 `tokio_rustls::LazyConfigAcceptor`**：只解析 ClientHello（拿到 SNI/ALPN）而不立即完成握手，这样可以先做异步操作（查证书缓存/现场签发）再决定用哪个 `ServerConfig` 完成握手——这是 `rustls` 生态本身提供的标准 API，我们直接用，不需要 hudsucker 包一层。
- **`RequestOrResponse` 枚举**（放行请求继续转发 / 直接短路返回响应）这种"处理结果二选一"的建模方式，值得在我们自己的 `RuleEngine` 输出类型里复用同样的语义。
- **tunnel-in-tunnel**：TLS 握手完成后，内层再走一遍完整的 HTTP 代理逻辑（递归），这个架构思路是对的，我们自己实现时会采用同样的结构。

### 2.6 关键设计取舍总结表

| 维度 | mitmproxy | HTTP Toolkit | Martian/goproxy（Go） | 本项目方案 |
|---|---|---|---|---|
| TLS 库 | pyOpenSSL | Node 内建 tls | Go 标准库 `crypto/tls` | **rustls**（自己驱动 `LazyConfigAcceptor` 做 SNI 探测+动态证书） |
| 证书生成 | cryptography 手写 | @peculiar/x509 | 标准库 `crypto/x509`，自研 map 缓存 | **rcgen**（自己写签发策略/缓存/CA 生命周期管理） |
| HTTP/1 | h11 | Node 内建 | 自研 `http1parser`（goproxy） | **自研状态机**（保留原始 header 顺序/大小写，可选复用 `httparse` 做底层 tokenizing） |
| HTTP/2 | hyper-h2 | Node 内建 + httpolyglot | 识别 `PRI` 前导码后转专用 `H2Transport` | **`h2` crate 帧级 API + 自研状态机**（自己组装事件模型，保留帧级可观测性） |
| HTTP/3 | aioquic（支持，复杂）| 不支持 | 不支持 | **quinn+h3，二期特性**（生态仍 0.0.x 不稳定），MITM 拦截 v1 不做 |
| WebSocket | wsproto | ws | 检测 101 响应后转裸字节双向转发 | **`tokio-tungstenite` 编解码 + 自研转发/拦截逻辑** |
| 协议探测/多路复用 | Layer 组合 + NextLayer | httpolyglot peek 首字节 | peek 1 字节（`0x16`）判断 TLS | **自研**：peek 首字节区分 TLS/明文/WS，思路参考上述两者 |
| 插件/规则架构 | Addon Manager（反射）| Rule/Handler 对象 | Handler 接口/回调函数 | **自研 Trait 对象**（`RuleEngine`/`InterceptHandler`，签名参考 hudsucker `HttpHandler` 但自己实现） |
| Flow 模型 | 可序列化、可 intercept/resume | 事件流 | 无内建 Flow 概念，需自己扩展 | **自研** struct + serde，`tokio::sync::oneshot` 做断点暂停/恢复 |
| 系统集成 | 有限 | **最全面**（代理/证书/ADB/Docker/Frida）| 无 | 参照其 `interceptors/` 分层设计逐步实现（这部分是系统集成脚本，与"是否用第三方 MITM 库"无关，本来就要自己写） |

**结论**：无论是 Python（mitmproxy）、Node（HTTP Toolkit）、Go（Martian/goproxy）还是 Rust（hudsucker），成熟的开源 MITM 实现无一例外都是**自己实现 accept/协议探测/CONNECT 隧道/HTTP 状态机这条核心链路，只在 TLS 握手本身和证书密码学签名这两个环节复用语言标准库或专门的密码学库**。没有任何一个项目把整条代理链路外包给另一个更上层的黑盒库。这与我们的自研决策方向完全一致，是行业惯例而非特例。

---

## 3. Rust 生态技术选型详细调研

### 3.1 HTTP 客户端引擎：reqwest vs hyper vs ureq

| 维度 | reqwest 0.13.x | hyper 1.x | ureq 3.x |
|---|---|---|---|
| 定位 | 高层客户端（基于 hyper） | 底层协议实现 | 同步阻塞极简客户端 |
| HTTP/2 | ✅ | ✅ | ❌ |
| HTTP/3 | ✅（`http3` feature，基于 h3+quinn，unstable）| ❌ | ❌ |
| 原始 Header 控制 | 中等（`http::HeaderMap` 按插入顺序序列化，够用）| 完全控制 | 有限 |
| Cookie Jar / 重定向策略 | ✅ 内建可自定义 | 需自实现 | ✅ |
| 精确计时（DNS/连接/TLS/TTFB） | 无内建，需自定义 Connector/中间件 | 可精确采集（直接控制各阶段） | 有限 |
| System Proxy | ✅ | 经 hyper-util | 部分 |

**结论**：API 客户端发送引擎用 **reqwest** 为主力（JSON/cookie/redirect/multipart/压缩全部开箱即用）。当 UI 需要展示逐阶段精确耗时（DNS/Connect/TLS/Send/TTFB，对标 Charles/DevTools 面板）时，用 **hyper + hyper-util** 直接手撸一个"检测客户端"打点——这套底层代码正好也是 MITM 代理部分本来就要写的东西，可以复用。ureq 因为同步阻塞、与我们整体 Tokio 异步架构不匹配，不予采用。

### 3.2 MITM 代理核心：完全自研（不依赖 hudsucker 等第三方封装库，详见 2.5 节）

### 3.3 TLS：rustls vs native-tls/openssl

**结论：MITM 代理必须用 rustls**。原因：纯 Rust、跨平台一致（不依赖系统 OpenSSL 版本，Tauri 打包更简单），且 `rustls` 本身提供的 `ResolvesServerCert`/`LazyConfigAcceptor` 等 API 能直接产出我们自己需要的 `ServerConfig`，用 native-tls 很难优雅实现"每域名单独证书+内存缓存+握手时动态查表"。这个动态证书缓存结构（如 `DashMap<String, Arc<ServerConfig>>` 或 `moka::Cache`）由我们自己维护。

动态 SNI 证书生成机制核心是 rustls 的 `ResolvesServerCert` trait + `tokio_rustls::LazyConfigAcceptor`（可以先读 ClientHello 再决定证书，比标准 `SNICallback` 更灵活，能在读到 ClientHello 后先做异步操作如查缓存/生成证书）。

注意：API 客户端引擎（发起请求给真实服务器）可以像 Yaak 一样**双 TLS 后端**（rustls 默认 + native-tls 可选回退），兼容用户环境里已经存在的企业自签名 CA/系统信任库场景；但 MITM 代理内核本身统一用 rustls。

### 3.4 证书生成：rcgen

Rust 事实标准（`rustls` 组织维护，0.14.x），只负责密码学级别的证书编码/签名，何时签发、给谁签发、缓存策略全部由我们自己的 `cuckoo-ca` 模块控制。核心 API：`CertificateParams`（SAN/有效期/KeyUsage/`use_authority_key_identifier_extension`）、`Issuer::from_ca_cert_pem` 加载已有 CA、`params.signed_by(&leaf_key, &issuer)` 签发、`CertificateParams::self_signed()` 生成根 CA。

根 CA 生成一次性完成并持久化到磁盘（否则每次重启都要求用户重装信任 CA）：

```rust
let mut params = CertificateParams::default();
params.distinguished_name = /* CN=Cuckoo Root CA, O=Cuckoo */;
params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
let key_pair = KeyPair::generate()?;
let ca_cert = params.self_signed(&key_pair)?;
// 持久化 ca.pem / ca.key 到应用数据目录
```

### 3.5 数据存储：SeaORM（直接用完整异步 ORM，不用 rusqlite + sea-query 手搓查询层）

**结论：数据存储层直接使用 `sea-orm`**（完整的异步 ORM，内部内嵌 `sea-query` 作为查询构建层），而非 `rusqlite`（同步驱动）+ `r2d2`（连接池）+ 裸 `sea-query`（仅类型安全查询构建，不映射实体）的组合。理由：

1. **异步一致性**：我们的 Rust 核心从 `cuckoo-http`（reqwest）、`cuckoo-proxy`（自研 tokio 引擎）到 Tauri 命令层全部是 `async`/tokio 运行时，`rusqlite` 是同步阻塞 API，必须配合 `r2d2` 连接池 + `tokio::task::spawn_blocking` 才能不阻塞 tokio 调度器，这是一层额外的心智负担和样板代码。`sea-orm` 默认基于 `sqlx-sqlite` 驱动，原生异步、原生 `tokio` 集成，`.await` 直接可用，去掉了这层胶水代码。
2. **减少手写样板代码**：`sea-query` 只提供类型安全的 SQL **构建**（`Query::select().from().columns()...`），行到结构体的映射、`INSERT`/`UPDATE` 的 `ActiveModel` 变更追踪、关联查询（`find_with_related`）等都要自己写。`sea-orm` 在 `sea-query` 之上补齐了完整的 Entity/Model/ActiveModel 三件套，`DeriveEntityModel` 宏从表结构自动生成大部分样板代码，Workspace/Folder/Request/Environment/Flow 这几张核心表之间的一对多/多对多关系（如 `Collection has many Requests`）用 `Related`/`RelationTrait` 声明后可以直接 `find_with_related()` 一次性加载，不需要手写 JOIN 再手动分组。
3. **迁移体系自带**：`sea-orm-migration` 提供标准的 up/down 迁移框架（`MigratorTrait`，按版本号顺序执行/回滚），比我们自己维护一套"手写 SQL 迁移脚本 + 版本号表"的方案更省心，且和 Entity 定义共享同一个 `sea-query` DSL，不用在 schema DDL 和 Rust 类型定义之间手动保持同步。
4. **对 ts-rs 类型共享无冲突**：`sea-orm` 的 `Model`/`ActiveModel` 只是普通 Rust struct，一样可以在 API 层用一个精简过的 DTO struct（去掉 ORM 内部字段）打 `#[derive(TS)] #[ts(export)]`，或者直接给 `Model` 派生 `Serialize` + `TS`（字段能对齐的情况下），不影响之前"Rust 是唯一类型真源、自动生成 TS 类型"的既定方案。
5. **代价与取舍**：`sea-orm` 编译期宏展开更重（增加编译时间），且对"手写高度定制化 SQL 做性能极致优化"的场景不如原生 `rusqlite`/`sea-query` 灵活——但我们的场景（Workspace/Collection/Request/Environment 等中小规模关系型数据 + Flow 元数据检索）远没有到需要手写 SQL 优化的量级，SQLite 本身单机单用户场景下 `sea-orm` 的额外开销可以忽略。如果未来 Flow 抓包记录量级变得非常大（数十万条以上）导致查询变慢，可以针对 `cuckoo-flow` 这一张高频写入表单独下沉到手写 SQL/`sqlx::query!` 宏，其余业务表继续用 `sea-orm`，两者可以在同一个 `sea-orm` 连接池（`DatabaseConnection` 内部持有的 `sqlx::SqlitePool`）上混用，并不互斥。

**具体依赖与配置**：

```toml
# cuckoo-store/Cargo.toml
sea-orm = { version = "2", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros", "with-chrono", "with-json"] }
sea-orm-migration = { version = "2" }
```

（实现阶段以 crates.io 上的实际最新稳定版本号为准，写文档时 SeaORM 官方文档显示的是 2.0.x 系列）

选 `runtime-tokio-rustls` 而非 `runtime-tokio-native-tls`：虽然 SQLite 是本地文件数据库、TLS 特性实际用不上，但保持与项目其余部分（MITM 内核用 `rustls`）统一的 TLS 后端可以少引入一份 OpenSSL/native-tls 的系统依赖，减小最终安装包体积和跨平台构建的不确定性。

**SQLite 具体配置沿用之前的调研结论不变**：仍然开启 **WAL 模式**（`PRAGMA journal_mode=WAL`，通过 `sea-orm` 连接选项或连接后执行一次 PRAGMA 语句设置）以支持读写并发，大 body/附件仍建议隔离到单独的 SQLite 文件或直接落盘为普通文件（数据库只存路径引用），避免大二进制内容拖慢主库的 WAL 文件增长和 VACUUM 效率。

### 3.6 HTTP/2 帧级访问：h2 crate

`h2`（hyperium 维护，0.4.x，与 hyper 共享作者）是事实标准，hyper 内部就用它实现 HTTP/2。因为我们不用 hyper 的高层封装作为代理转发层（那样看不到帧），而是直接驱动 `h2` 的帧级 API，所以从第一天开始就具备"像 Charles 一样的 HTTP/2 帧时间线"的能力基础，只是 UI 层面的帧级可视化界面可以**列为 v2+ 里程碑，不阻塞 MVP**（MVP 阶段先把帧事件聚合成整请求/整响应展示，多数用户对帧级视图无刚需）。

### 3.7 HTTP/3 / QUIC —— 最大技术风险点

| crate | 版本 | 状态 |
|---|---|---|
| `quinn` | 0.11.x | ✅ 成熟，生产可用 |
| `h3` | 0.0.8 | ⚠️ 版本号仍 0.0.x，API 不稳定 |
| `h3-quinn` | 0.0.10 | ⚠️ 同上 |

**作为客户端**（我们主动发 h3 请求）：可行，quinn 成熟，h3 能跑通基本 GET/POST，但 API 会随小版本破坏性变更。

**作为 MITM 拦截**：真正的难题。QUIC 把 TCP 握手+TLS 握手合并进基于 UDP 的加密握手，没有明文 SNI 阶段可"看一眼再路由"（需要解密 QUIC Initial 包，其密钥可从版本号公开派生，mitmproxy/aioquic 正是这么做）。必须同时实现双向完整 QUIC 协议栈（面向客户端是假服务器，面向真实服务器是假客户端）。此外许多 HTTP/3 客户端在证书校验失败时会**静默降级到 h2/h1** 而非报错，容易让用户误以为拦到了 h3 流量。

**结论：MVP 明确不做 HTTP/3 的 MITM 拦截**，只做 HTTP/1.1 + HTTP/2 + WebSocket 的 MITM（覆盖 90%+ 调试需求）。HTTP/3 仅作为 API 客户端主动发送功能提供（标注 Beta）。MITM 层面的 HTTP/3 支持列为远期独立子项目。

### 3.8 WebSocket：tokio-tungstenite

事实标准（234M 下载）。代理场景下我们自己的代理内核直接用它做帧编解码，双向转发、拦截、修改、丢弃帧的业务逻辑全部自己写。纯客户端模式（我们自己连 WS 服务器调试，不经过代理）下用 `tokio_tungstenite::connect_async` 建连，Tauri command 发送帧、event/channel 接收帧。

### 3.9 GraphQL：无需专门协议支持

GraphQL 本质是 HTTP POST + JSON body（`{"query", "variables", "operationName"}`），传输层与普通 HTTP 请求完全一致。MITM 拦截层面我们自研的 HTTP 拦截能力已完全覆盖，无需协议栈改动。唯一需要做的是**前端展示层**识别（按 URL 路径或 body 里的 `query` 字段）并提供语法高亮/query 树状展示/variables 格式化。GraphQL Subscription（基于 WS 的 `graphql-ws`/`graphql-transport-ws` 协议）复用第 3.8 节 WS 拦截机制，UI 层解析 `connection_init`/`subscribe`/`next`/`complete` 消息类型即可。

### 3.10 系统代理配置——无统一 crate，需手写分平台胶水代码

| 平台 | 方案 |
|---|---|
| macOS | Shell out `networksetup -setwebproxy/-setsecurewebproxy <服务名> <host> <port>`；也有 `system-configuration` crate 可读取配置，但写入需要 `SCPreferences` + 授权弹窗，比 shell out 复杂得多，MVP 建议直接 shell out |
| Windows | 修改注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`（`winreg` crate）+ 调用 WinAPI `InternetSetOptionW` 通知系统刷新代理 |
| Linux | 无统一方式：GNOME 用 `gsettings set org.gnome.system.proxy ...`；KDE 用 `kwriteconfig5`；需检测 `$XDG_CURRENT_DESKTOP` 分支处理，对无法识别的环境提示用户手动配置 |

**必须做**应用退出/崩溃时自动恢复系统代理设置的兜底逻辑（注册 panic hook + 正常退出 hook），否则用户断网体验会很差——这是所有同类工具的常见坑。

### 3.10.1 按进程过滤流量——系统代理模式做不到，需要额外的"进程归因"手段

这是一个实际使用中很常见的需求（"只抓某个 App/某个命令行工具的包，不想看到浏览器/系统服务的噪音流量"），但**必须先厘清一个关键事实：无论是显式系统代理还是 TUN/WireGuard 模式，代理服务器收到一个 TCP 连接时，操作系统默认都不会告诉你这个连接是哪个进程发起的**——HTTP 协议本身、TCP 三次握手本身都不携带进程身份信息，这是操作系统网络栈的设计使然，不是哪个代理工具"没做"，而是这一层信息需要额外反查。归因手段分两类：

**手段一：连接层面反查"哪个本地进程持有这个 socket"（事后关联，不影响流量路径）**

代理收到连接后，能拿到的是 `(本地IP, 本地端口, 远端IP, 远端端口)` 四元组（对显式代理来说是客户端到代理这一跳的四元组；对透明代理/TUN 模式是原始四元组）。用这个四元组反查本地进程是纯操作系统 API 层面的能力，与代理内核实现无关：

| 平台 | 具体机制 |
|---|---|
| macOS | 私有但常用的 `libproc` API：`proc_pidinfo(pid, PROC_PIDLISTFDS, ...)` 遍历所有进程的 fd 表找 socket，或反过来用 `PROC_PIDLISTFDS`+`sysctl(NET_RT_TCP)` 拿到 "四元组→pid" 映射（`lsof`/`nettop` 内部用的就是这套）。Rust 生态有 `netstat2`/`sysinfo` crate 封装了跨平台版本，但 macOS 上仍是走同一套私有 API，无需 root（普通用户权限即可查看自己的连接，查看其他用户进程需要额外权限） |
| Windows | 官方公开 API `GetExtendedTcpTable`/`GetExtendedUdpTable`（`iphlpapi.dll`），直接返回带 `dwOwningPid` 字段的连接表，是四个平台里**最简单直接**的一个，`windows`/`netstat2` crate 都有现成封装 |
| Linux | 读 `/proc/net/tcp`（`sl local_address rem_address st ... inode`）拿到 socket 的 `inode`，再遍历 `/proc/*/fd/*` 找哪个进程的 fd 是指向这个 `inode` 的 `socket:[inode]` 符号链接——这是 `lsof`/`ss -p` 的实现原理，本质是**遍历+暴力匹配**，进程数多时有一定开销，但对代理场景（只需查偶尔新建的连接）完全够用 |

这套方案的关键限制：**只能查"谁在跟我的代理进程通信"，也就是发起方是本机进程时才能查到**——如果流量来自局域网内的手机/其他设备（这是抓包工具的常见场景！），操作系统层面根本没有"进程"概念可言，这条路完全失效，只能退化到按设备 IP 过滤。

**手段二：在流量重定向阶段就按进程做过滤（系统级"透明分流"，Little Snitch/Surge/Stash 这类工具的路子）**

- **macOS**：`NetworkExtension.framework` 的 **Content Filter Provider** 或 **App Proxy Provider**（跟 mitmproxy_rs 用的是同一套系统扩展框架，见 2.3 节）在内核建连阶段就能拿到 `NEFilterFlow.sourceAppSigningIdentifier`/审计令牌，可以按 bundle ID 白名单/黑名单决定这条连接要不要重定向进代理——这是 Surge/Charles for Mac "按 App 分流"功能背后的真实机制，但**需要 Apple 签名的 Network Extension 权限（`com.apple.developer.networking.networkextension`），个人开发者账号可以申请，公证要求比普通 App 更严格**，工程成本明显高于系统代理方案。
- **Windows**：WFP（Windows Filtering Platform）回调函数里可以拿到 `FWPM_CONDITION_ALE_APP_ID`（发起连接的可执行文件路径），这也是 `mitmproxy-windows` 用 WinDivert（基于 WFP 的开源封装）而不是简单端口重定向的原因之一——`mitmproxy_rs` 的 `redirector` 子 crate 支持"只重定向某些进程"的过滤规则（见 2.3 节的 workspace 结构）。
- **Linux**：cgroup v2 net_cls / eBPF `cgroup/connect4` 挂载点可以在 socket connect 系统调用层面按 cgroup（进而按启动该进程的容器/systemd unit）过滤，`mitmproxy-linux-ebpf` 用的正是这条路；对没有走 cgroup 隔离的普通进程，需要更底层的 `bpf_get_current_pid_tgid()` 在 eBPF 程序里直接拿 pid 做过滤，工程复杂度全平台最高。
- **移动端**（手机是抓包的主要目标场景之一）：iOS 只能通过 **NEPacketTunnelProvider（个人 VPN）+ 系统"按 App 使用 VPN"配置**（需要 MDM/监督模式，普通消费者场景基本用不了）；Android 从 5.0 起 `VpnService.Builder` 原生支持 `addAllowedApplication()`/`addDisallowedApplication()`，是四个场景里**唯一对普通用户友好、无需 root 就能做到"按 App 抓包"**的平台。

**本项目建议（务实路线）**：
1. **MVP**：只做手段一（连接四元组反查本机进程），在 Flow 列表里给每条记录标注"发起进程名/PID/可执行文件路径"作为**展示和筛选用的元数据**，不改变任何流量路径。这个成本很低（跨平台 API 都不复杂），对"本机调试自己的某个后端服务/CLI 工具"这个高频场景完全够用，也是 HTTP Toolkit/Proxyman 等竞品的实际做法（它们同样没有做手段二那种内核级按进程分流）。
2. **不做承诺的方向**：手段二（系统扩展级别的按进程分流）工程成本和签名/权限门槛都显著更高，且主要解决的是"跨设备抓包时如何免配置只抓某个 App"这个更细分的需求，与我们 MVP 阶段"系统代理够用"的判断一致，列为远期方向即可，不阻塞当前计划。
3. **安卓场景特殊处理**：如果未来做配套的抓包助手 App（参照 HTTP Toolkit 的 Android 支持），`VpnService` 的 `addAllowedApplication` 是现成的、成本最低的"按 App 抓包"实现，比在 Rust 桌面端做任何事情都更直接。

**跨设备场景：如果手机上也装一个我们的配套 App，能否反查是哪个 App 产生的流量？**——这个思路本质是**把"进程归因"这件事从桌面端下放到手机本地做，再把结果回传给桌面端跟 Flow 关联**，可行性因平台而异，差异巨大：

- **Android：完全可行，且是四个场景里最容易实现的**。配套 App 用 `VpnService` 建一个本地 loopback VPN（不需要 root，`addAllowedApplication`/`addDisallowedApplication` 甚至可以精确控制只对哪些 App 生效），拿到 IP 层数据包后，可以用两条路径查询发起方 UID/包名：
  1. **官方公开 API**（Android 10 / API 29+）：`ConnectivityManager.getConnectionOwnerUid(protocol, local, remote)`，直接传四元组换 UID，再用 `PackageManager.getPackagesForUid(uid)` 换出包名——**这是最简单直接的路径**，跟桌面端 Windows 的 `GetExtendedTcpTable` 思路一模一样，本质都是操作系统提供的"连接四元组→身份"反查接口，不需要解析数据包内容。
  2. VPN 服务本身能看到的是**同一个 App 内所有请求共享同一个 VPN tun fd**，需要额外做的是：对每个新建立的 TCP/UDP 四元组调一次 `getConnectionOwnerUid`（有一定延迟和调用开销，需要做缓存），而不是每个包都查。
  3. 查到包名/UID 后，手机 App 通过一个轻量本地通道（比如 WebSocket，或者直接复用同一条 mTLS/WireGuard 隧道边带一个控制通道）把 `"四元组 → 应用包名"` 的映射实时推送给桌面端，桌面代理收到对应 Flow 时按四元组查表标注即可——这跟桌面端"手段一"的实现思路完全对称，只是反查动作发生在手机本地而不是桌面本地。
  4. **这其实是市面上 PCAPdroid、NetGuard 等安卓抓包 App 的标准做法**，技术路线是成熟的，唯一的额外工作量是"手机 App 和桌面 App 之间设计一个同步协议"。

- **iOS：几乎不可行（非越狱情况下）**。iOS 的 App 沙箱严格禁止一个 App 获取"系统里其他 App 的信息"（既不能列举其他 App，也不能反查连接归属进程），`NEPacketTunnelProvider` 能拿到的只是原始 IP 包和该 VPN Profile 对应的**整个系统的流量**（iOS 的"按 App 使用 VPN"是**系统按配置描述文件路由流量到 VPN**，而不是"VPN 内部能查出是哪个 App 发的"——这是两码事，前者在 MDM/监督模式下才能配置，后者 API 层面 iOS 压根没开放给第三方 App）。唯一的理论例外是**越狱设备**（能 hook `libnetcore`/直接读内核网络状态表），但这已经超出普通产品能覆盖的用户群体，不建议作为产品能力规划。

**本项目建议补充**：如果规划移动端配套 App，**Android 优先做"手机本地进程归因 + 回传桌面端关联"这个能力**，性价比很高（复用系统 API，不需要 root，用户体验类似"电脑上直接看到是手机上哪个 App 发的请求"）；iOS 端只能做到"按域名/IP/证书信息展示"，进程级别的归因如实告知用户做不到，不要在 iOS 上强行承诺这个能力。

### 3.10.2 系统代理 vs GotaTun/TUN 模式：性能对比

这个问题本质是在问"**用户态应用层转发（system proxy）**"和"**内核旁路 + 用户态协议栈重组（WireGuard/TUN + smoltcp，见 2.3 节）**"这两种取得流量的方式，谁的开销更大。结论是：**系统代理模式几乎总是更快，TUN/WireGuard 模式的额外开销来自"需要在用户态重新实现一遍 TCP/IP 协议栈"这个本质工作量，而不是 GotaTun 本身慢**。具体拆解：

**系统代理模式的数据路径**（我们当前 MVP 采用的方案）：
```
客户端进程 → (系统网络栈处理一次) → TCP连接到代理监听端口
           → 代理进程用户态收到已经是干净的字节流（内核已经完成 TCP 分段重组/乱序重排/重传）
           → 代理转发到真实服务器（走系统网络栈第二次）
```
关键点：**TCP/IP 协议栈的脏活——分段、重组、拥塞控制、重传、乱序处理——完全由操作系统内核完成**，代理进程拿到的是应用层已经能直接 `read()` 出连续字节流的 socket。代理本身只做"字节从一个 socket 转发到另一个 socket"，这是内核高度优化过的路径（甚至可以用 `splice()`/`sendfile()` 做零拷贝，虽然 TLS 终止场景下用不上，因为需要过一遍加解密）。额外开销主要是**多了一跳 TCP 连接**（客户端→代理→服务器 而非 客户端→服务器），带来的是连接建立的握手延迟（多一次 RTT 量级，通常在同机/局域网内可忽略）和内核态两次协议栈处理的 CPU 开销，但这个开销级别是"正常做一次代理"的开销，几十年来所有正向代理都是这个模式，非常成熟、优化得很好。

**GotaTun/TUN 模式的数据路径**（mitmproxy_rs 的 WireGuard 模式，见 2.3 节）：
```
客户端进程 → 系统路由到 TUN 虚拟网卡 → 内核把原始 IP 包丢给用户态 TUN 读取者
           → GotaTun 解密 WireGuard 封装（如果走 WireGuard 隧道；纯 TUN 模式没有这层）
           → smoltcp 在用户态"重新实现"一遍 TCP/IP 协议栈：
             解析 IP 包头、TCP 包头，做序列号跟踪、乱序重排、ACK 生成、重传定时器、拥塞窗口……
           → smoltcp 组装出应用层字节流交给上层代码
           → （反向）应用层数据要发出去时，smoltcp 再把字节流重新切成 TCP segment、算校验和、生成 IP 包
           → 加密（WireGuard 模式）→ 写回 TUN 设备 → 内核路由发出
```
关键点：**这条路径把操作系统内核原本免费做好的 TCP/IP 协议栈处理，重新在用户态跑了一遍**（`smoltcp` 存在的意义正是"帮你在用户态重新实现一个精简 TCP/IP 协议栈"）。额外开销来源，按量级排序：
1. **内核态-用户态数据拷贝次数翻倍**：正常 TCP 收发只有 1 次内核缓冲区到用户态缓冲区的拷贝；TUN 模式下数据要先从"真实网卡"到内核，再拷贝到 TUN 设备的用户态读取端（这已经是一次跨界），`smoltcp` 处理完还要再拷一次给上层业务逻辑，本质上是多了一趟"用户态搭了一个影子协议栈"的完整往返。
2. **TCP 状态机在用户态重新计算**：序列号比较、SACK 处理、RTT 估计、拥塞控制算法（`smoltcp` 实现的是精简版，不如 Linux/XNU 内核 TCP 栈经过几十年优化的实现高效），这些是纯 CPU 开销，量级上通常是"每包多几百纳秒到微秒级"的计算，在高吞吐场景（比如抓一个正在下载大文件的连接）会累积成可观的 CPU 占用。
3. **WireGuard 加解密本身**（`GotaTun`）：ChaCha20-Poly1305 在现代 CPU 上有 AES-NI 类似的高速路径，这部分本身很快（软件 WireGuard 实测吞吐能到单核 1GB/s+ 量级），**不是瓶颈**，除非目标是长期跑满带宽的场景。
4. **额外的一层封装开销**（如果走 WireGuard 隧道）：UDP 封装 + 加密带来的包头膨胀（~60 字节/包），对小包密集的场景（大量小 HTTP 请求）相对损耗比例更高。

**综合结论**：
- 对于我们的核心场景（**代理网页/App 的 HTTP(S) 流量做调试查看**），**系统代理模式的性能明显优于 TUN/WireGuard 模式**，因为前者完全复用了内核高度优化的协议栈，只是多转发一跳；后者需要在用户态重新实现协议栈，多了一整套 CPU 密集的协议处理逻辑。
- GotaTun/TUN 模式真正的价值**不是性能**，而是**免配置的流量接入能力**（手机不需要手动设置代理，接入 WireGuard 配置即可全局生效；能拿到 UDP/非 HTTP 协议流量；能做到设备级别而非仅 HTTP 客户端级别的拦截）——这是用可观的性能/工程成本换"用户体验更友好的接入方式"，mitmproxy 团队自己也是先有稳定的系统代理方案多年后，才补上 WireGuard 模式作为**移动端免配置抓包**的补充方案，而非替代。
- 这个判断进一步验证了 investigation.md 2.3 节和 3.10 节已有的结论：**MVP 阶段用系统代理已经足够，且性能表现会更好**；WireGuard/TUN 模式留作远期"移动端免配置抓包"的可选增强能力，实现时应明确告知用户"这个模式会有额外的 CPU 开销，追求性能调试大文件传输场景建议用系统代理模式"。

### 3.11 根 CA 信任链安装——同样无统一 crate

`rustls-native-certs` 只能**读取**系统信任的 CA 列表（给 rustls 客户端用作验证锚点），不能**写入**安装新 CA。各平台写入方式：

| 平台 | 机制 |
|---|---|
| macOS | `security add-trusted-cert -d -r trustRoot -k <keychain> ca.pem`（系统级需要 sudo/管理员授权），Firefox 走独立 NSS 库需额外处理 |
| Windows | `certutil -addstore -f "ROOT" ca.crt`（需管理员权限） |
| Linux | Debian/Ubuntu: `update-ca-certificates`；RHEL/Fedora: `update-ca-trust`；Firefox/Chromium 在多数发行版走 NSS（`~/.pki/nssdb`），需要 `certutil -d sql:$HOME/.pki/nssdb -A -t "C,," ...`（`libnss3-tools`） |

**mitmproxy 的取巧做法**：不主动安装，而是启动特殊域名 `mitm.it`，用户手动访问下载证书按平台向导安装，**把责任交给用户**，规避跨平台自动安装的复杂度和权限审查问题（尤其 macOS 应用公证、Windows SmartScreen 对"写入系统信任库"这类行为格外敏感）。

**本项目建议**：v1 采用 mitmproxy 式的"引导式安装向导"（提供下载证书 + 分平台图文/一键命令说明），v2 再考虑加"一键安装"（调用系统提权对话框，如 macOS `osascript ... with administrator privileges`）。无论哪种方式，都要把"移除 CA"做成一等公民功能。

### 3.12 唯一通信协议：HTTP（请求-响应）+ SSE（服务端推送），桌面 UI 不特殊化

**背景**：项目天然存在三类客户端——桌面 UI（Tauri WebView）、CLI、MCP Server（供 AI Agent 调用）。如果桌面 UI 走 `tauri::ipc::Channel`/`invoke` 这条 Tauri 专属路径，而 CLI/MCP 走另一条网络协议路径，就会出现**两套并行协议、两套事件模型、两套鉴权机制**，每新增一个 Service 能力都要在两条路径上各写一遍适配代码——这正是"胶水代码"的来源。既然 CLI/MCP 已经决定要用本地 Server（论证见下文），更彻底的做法是**让桌面 UI 也作为这个 Server 的一个客户端**，三类客户端共用同一份协议实现。

**为什么"Yaak CLI 直接打开同一个 SQLite 文件"这条捷径对我们不成立**：Yaak 的 `crates-cli/yaak-cli`（1.1 节）之所以能"零成本复用"，是因为它面对的主要是**静态数据的 CRUD**（Collection/Request/Environment 都是存量数据，随时可以直接读写同一个 SQLite 文件，不涉及运行时状态）。我们的场景多了一层本质区别：**MITM 代理是一个有状态的长驻进程**——它是否在监听、监听在哪个端口、当前有哪些请求正卡在断点等待放行、实时产生的 Flow 事件流——这些都是**运行时内存状态**和**持续产生的事件流**，根本不落盘在 SQLite 里（落盘的只是"最终确定"的 Flow 记录，见 3.3 节 body 惰性加载的设计）。如果 CLI/MCP 只是"另开一个进程读同一个 SQLite"，它们看不到"代理有没有在跑""现在断点卡住了哪些请求""这一秒内又来了哪些新流量"——而这些恰恰是最需要被 AI/CLI 感知和操作的能力。这确认了"需要一个运行时能力入口"这个前提，接下来的问题是这个入口该长什么样、用什么协议、桌面 UI 要不要也走这条路。

**协议选型的调研过程：WebSocket 并不是这个场景下的业界主流选择**。最初考虑过 WebSocket + JSON-RPC 2.0（双向、单连接、可同时承载请求-响应和推送），但进一步调研同类场景的真实产品架构后发现，更贴近的先例几乎都不是这么做的：

- **OpenCode**（TUI + Desktop + Web + VS Code 四种客户端接入同一个 Server，与我们的场景高度对应）：采用 **HTTP API（客户端发指令）+ SSE 端点 `/global/event`（服务端推送事件）**，官方定位是"任何能发 HTTP 请求的客户端都可以接入"（OpenAPI 兼容），多客户端通过 SSE 实现实时状态同步，不使用 WebSocket。
- **Claude Code 的 Server 模式**（供 VS Code 扩展等 IDE 集成调用）：本地起一个 HTTP API 服务器，IDE 扩展通过标准 HTTP 调用；官方数据显示本地 HTTP 延迟通常 < 10ms，优于经云端中继的 Bridge 模式。同样不使用 WebSocket。
- **OpenAI Codex 的 `app-server`**（同时服务 VS Code 扩展、CLI、桌面应用）：协议是 JSON-RPC 2.0，但默认 transport 是 **stdio**（进程管道，VS Code 扩展场景），WebSocket 只是可选 transport 之一而非首选。
- **MCP 协议本身**：官方在制定远程通信标准时认真评估过 WebSocket，最终决定采用 **Streamable HTTP**（HTTP POST 发消息，服务器可选择将响应升级为 SSE 流）而非 WebSocket，理由直接构成对我们场景的参考：
  1. 我们的大部分操作（发请求、CRUD Collection、启停代理）本质是一次性的 RPC 语义，为此维护一条长连接的握手/心跳/重连成本不成比例；
  2. **浏览器环境下 WebSocket 握手阶段无法附加标准 `Authorization` 请求头**，鉴权只能退化成 URL query 参数等变通方案；而 HTTP 请求可以直接用标准 header 鉴权，是更干净、更符合惯例的做法（这一点后续也影响了我们的鉴权设计，见 3.14 节）；
  3. 只有 HTTP GET 能被浏览器自动升级为 WebSocket，POST 不行，等于要为"客户端发指令"这类操作专门设计一套升级流程，不如直接用 POST 语义清晰；
  4. 避免"客户端和服务器 transport 组合过多"带来的兼容性负担——单一 HTTP 协议比强制所有客户端都实现一个 WS 客户端更简单。

**结论：`cuckoo-server` 对外的唯一协议是 HTTP（请求-响应）+ SSE（服务端主动推送）**，不引入 WebSocket：

1. **请求-响应式操作走标准 REST 语义**（`POST /api/requests/send`、`GET /api/flows` 等），浏览器原生 `fetch`、CLI 的 HTTP 客户端库、`curl` 手动调试都直接可用，鉴权走标准 `Authorization: Bearer <token>` 请求头。
2. **服务端主动推送走 SSE**（`GET /api/flows/stream`，`Content-Type: text/event-stream`），浏览器原生 `EventSource` API 支持自动重连；CLI/MCP 场景下用一个轻量 SSE 解析客户端（本质是按行解析 `text/event-stream` 格式，不需要额外依赖重的库）。

**为什么桌面 UI 的业务 API 调用也应该走这个入口，而不是保留 Tauri IPC 特化路径**：

1. **性能不构成真实瓶颈**——本地 loopback HTTP 请求的延迟量级同样是个位数到低两位数毫秒（Claude Code 官方数据佐证：本地 HTTP < 10ms），对于人眼可感知的 UI 交互和"每秒数百到数千条"流量记录的展示场景，这个开销完全淹没在渲染开销之内，不构成真实瓶颈。真正的性能关键点在于"批量聚合 + body 惰性加载 + 前端虚拟列表"这三件事（本节后半部分详述），与具体传输协议无关。
2. **两套协议的维护成本远高于统一协议的性能开销**——如果桌面 UI 走 `invoke`+`Channel`、CLI/MCP/手机走 HTTP+SSE，那么 Service 层每新增一个方法/事件，都需要同时想清楚"Tauri command 怎么包一层""REST 端点怎么包一层""事件类型在两边是否共用"，工程实践中这类"双轨制"是最容易出现遗漏、语义漂移的地方（参考 Bruno 手写两份 schema 漂移的教训，本质是同一类问题的不同表现形式）。统一成一条路径后，新增能力只需要在 Service 层写一次、在路由描述里声明一次，所有客户端自动获得，彻底消灭这一类胶水代码，详见 3.13 节的代码生成方案。
3. **Tauri 的角色彻底退化为"壳"，但页面资源加载方式仍走 Tauri 原生协议，与业务 API 协议是两回事**：桌面应用启动时，Rust 主进程内 `tokio::spawn` 一个 `cuckoo-server`（`axum` 提供 REST + SSE），专门服务**业务 API 调用**；但 `WebviewWindow` 本身**始终通过 Tauri 官方的 `tauri://` 自定义协议加载打包进二进制的前端静态资源**（`index.html`/JS/CSS），而不是让窗口本身也去请求 `http://127.0.0.1:<port>/` 这个 HTTP 地址去拿页面。这是一个重要的修正：早期方案曾设想"窗口直接加载 `http://127.0.0.1:<port>/`，`cuckoo-server` 顺带用 `axum` 托管前端静态资源"，图省事让桌面窗口和浏览器访问在协议层面完全一致；但这个设计不成立——**没有理由放弃 Tauri 官方提供的、更安全更高效的资源加载机制**：`tauri://` 协议下前端资源直接从打包的二进制资源里读取，不经过任何网络栈（无 TCP 握手、无 HTTP 解析开销），且享有 Tauri 2.x 专门为该协议设计的 CSP、隔离上下文等安全特性；改成 HTTP 加载页面后，这些安全特性全部损失，还平白多了一层"`cuckoo-server` 必须自己实现一个静态文件服务器"的负担，而这个负担对桌面场景毫无必要（桌面用户 100% 能从本地读到打包资源，不需要经网络下发）。**正确的边界划分是**：页面资源加载（HTML/JS/CSS，静态、不含用户数据、只服务本机窗口）走 `tauri://`，且是前端页面**唯一**的加载途径；业务数据/操作（发请求、订阅 Flow 事件等，动态、需要鉴权、需要被 CLI/MCP 等其他客户端复用）走 `cuckoo-server` 的 HTTP+SSE。前端页面加载完成后，页面内的 JS 代码用标准 `fetch`/`EventSource` 访问 `http://127.0.0.1:<port>` 上的 `cuckoo-server`——**"业务协议统一"这条核心原则不受影响，只是"页面外壳怎么加载"改用更合适的官方机制**。`cuckoo-server` **不提供任何静态文件托管能力**，也不存在"允许局域网/手机访问 UI"这个可选开关——前端页面完全随桌面应用分发，不需要也不存在通过网络访问 UI 页面的能力。`tauri::command`/`tauri::ipc::Channel` 不再承载任何业务能力，只保留极少数"必须由原生进程完成、且不适合暴露成网络可达能力"的系统级操作（系统托盘菜单事件、原生文件选择对话框的 UI 呈现、开机自启注册、**把 `server.token` 传递给前端 JS 这个初始化步骤**），这些操作依然通过 Service 层的 `system.*` 方法统一走同一套 REST 接口或一个极薄的 Tauri command（下文 3.13 节说明"实现在哪"与"协议在哪"是两回事）。

**高频事件的工程实践在协议确定后同样适用，只是载体从自定义消息换成 SSE 事件**：
1. **批量聚合**：Service 层内部用 `tokio::sync::broadcast`（有界）收集 Flow 事件，按 16-50ms 时间窗口或"攒够 N 条"双触发条件聚合成一批，序列化成一条 SSE `event: flow.batch` 推给所有已连接的 SSE 客户端，而不是逐条发送，显著降低消息数量和序列化次数。
2. **大 Body 惰性加载**：请求/响应的**元数据**（method/url/status/headers/timing）走高频小事件实时推送；**body 内容**不塞进事件里，客户端按需调用 `GET /api/flows/:id/body` 主动拉取——这条原则与传输协议无关，是所有 4 类客户端共享的设计。
3. **背压处理**：`broadcast` channel 满了自然丢弃最老数据（`broadcast` 的默认语义），SSE 连接侧检测到 lagged 后可以推一条特殊事件提示客户端主动重新拉取一次全量快照兜底，而非无限增长内存。
4. **SSE 断线重连**：浏览器 `EventSource` 原生自动重连，配合 SSE 标准的 `Last-Event-ID` 机制可以做到"断线后从上次收到的事件继续"，不需要自己实现重连逻辑——这是 SSE 相对自定义消息协议的一个开箱即用优势。
5. **本地环回网络吞吐验证**：`127.0.0.1` 上的 HTTP/SSE 往返吞吐量远超代理场景下真实可能出现的流量峰值（正常联调/抓包场景很难达到每秒上千次独立调用的量级，况且已经做了批量聚合），这个假设在实现阶段用真实压测验证即可，不构成方案选型层面的阻塞风险。

### 3.13 少写胶水代码：从单一 Service 定义自动生成路由 + 多端客户端

3.12 节确立了"一个协议（HTTP+SSE）、一个 Server、三类客户端"的骨架，本节解决用户提出的第二个关切——**新增一个能力时，如何避免要在 Service 函数、REST 路由、SSE 事件类型、TS 客户端、CLI 子命令、MCP tool 定义这几个地方分别写一遍重复的胶水代码**。

**核心思路：Service 层方法签名是唯一真源，其余客户端可见的接口形态全部由宏/构建脚本从它派生**，而不是手写多份声明后再人工保证一致：

1. **Service 方法用一个属性宏统一标注**（如 `#[rpc_method("POST", "/api/flows/:id/resume")]`），宏在编译期做两件事：（a）把该方法注册进一张路由表，`cuckoo-server`（`axum`）按此表自动生成 `Router`，新增方法不需要手写单独的 handler 注册代码；（b）把方法的入参/返回类型信息（本身已经是 `#[derive(Serialize, Deserialize, TS)]` 标注过的类型，见 3.5 节类型共享方案）收集进一份编译期可枚举的"方法清单"。
2. **TS 客户端由方法清单驱动生成**：一个构建脚本（`build.rs` 或独立的 codegen 二进制）遍历方法清单，为每个方法生成一段形如 `api.flows.resume(id, params)` 的强类型 `fetch` 封装函数，连同 `ts-rs` 已经产出的类型文件一起写入前端 `lib/api/generated.ts`；SSE 事件类型同样有对应的 TS 类型定义，前端订阅时按事件名分发到对应的处理函数。前端业务代码调用体验接近一个强类型的 SDK，但底层完全是我们自己维护的轻量生成器，不引入绑定特定后端语言的第三方方案（如 tRPC）。
3. **CLI 子命令**：CLI 的绝大多数子命令是"拼一个 HTTP 请求 + 打印格式化后的结果"这个模式的重复，用同一份方法清单驱动一个通用命令框架（`cuckoo call <method> <path> --json '{...}'` 作为兜底通用入口，覆盖长尾方法），对于少数需要更友好交互的高频命令（如 `cuckoo send`、`cuckoo flow list --follow`，后者内部是一个 SSE 客户端）手写更符合人体工程学的参数解析，但底层最终都调用同一个 HTTP/SSE 客户端库函数，不重复实现连接/鉴权/重试逻辑。
4. **MCP tool 定义**：MCP 的 tool schema（JSON Schema 描述的 input/output）同样可以从方法清单的类型信息批量派生（Rust 类型 → JSON Schema 有成熟 crate 如 `schemars`，且 `ts-rs` 标注的类型天然适合复用同一份类型定义），`cuckoo-mcp` 在启动时遍历"哪些方法标记为对 AI 可见"（并非所有内部方法都适合直接暴露给 AI，需要一个 `#[rpc_method("...", mcp_visible = true)]` 之类的显式开关），批量注册为 MCP tools，避免手写一份与 REST 方法平行的 tool 列表。MCP 协议自身的 Streamable HTTP transport 与我们的 HTTP+SSE 选型天然契合，`cuckoo-mcp` 可以直接复用同一套底层 HTTP 客户端逻辑对接 `cuckoo-server`。
5. **这套生成机制本身的复杂度要与项目体量匹配**：v1 阶段方法数量不多（几十个量级），可以先手写一份清单式的宏或者哪怕是简单的 `build.rs` 脚本读取一个方法描述表，不必一开始就追求"完全自动反射"的高级形态；核心原则是"新增能力只改一处源头定义"，具体生成手段可以随项目复杂度逐步演进（如后续引入更完整的 `schemars`/反射方案）。

**多端接入的统一体验**：

- **桌面 UI**：Tauri 窗口通过 `tauri://` 自定义协议加载打包好的前端静态资源（页面资源不经过网络，也是前端页面唯一的加载途径），页面加载完成后前端代码用标准 `fetch` 向 `http://127.0.0.1:<port>` 发起业务请求、`EventSource` 订阅 `/api/flows/stream`——页面外壳加载和业务 API 调用走两条不同的通道，但业务 API 这条通道与其余两类客户端（CLI、MCP）完全一致。
- **CLI**：`cuckoo` 二进制内置一个基于 `reqwest` 的 HTTP 客户端 + 一个轻量 SSE 行解析器，启动时连接本地 Server（若未运行则按 3.14 节策略拉起），执行完命令后断开。
- **MCP**：`cuckoo-mcp` 作为一个 MCP tool 分发器，内部持有一个到本地 Server 的 HTTP 客户端（或进程内直连 Service，见 3.14 节），AI Agent 调用 MCP tool 时转译成对应的 HTTP 请求。

`cuckoo-server` 自始至终只监听 `127.0.0.1`，不提供任何静态文件/前端页面的托管能力，也不存在"允许局域网/手机访问 UI"这类可选能力——前端页面完全随桌面应用分发，不需要也不支持通过网络访问 UI 页面。（注意：这与“代理拓捕手机/浏览器的 HTTP(S) 流量”是完全不同的事——后者是本产品的核心 MITM 代理能力，仍然保留，见 1.2 节。）

### 3.14 本地 Server 的生命周期与安全边界

1. **Server 生命周期**：`cuckoo-server`（`axum`）既可以由桌面应用启动时在同进程内 `tokio::spawn` 拉起（监听固定或可配置端口），也可以独立编译运行（`cuckoo-server --headless`）供"只想用 CLI/AI、不需要 GUI"的场景使用，两种方式内部持有的是同一个 Service 层实例，行为完全一致。CLI 在未检测到本地 Server 运行时，对于一次性命令可自动拉起一个短生命周期的 headless Server 完成请求后退出（类比 `ollama` CLI 自动拉起本地服务的模式）；对于需要持续连接的命令（如 `--follow`、`proxy start`），提示用户先执行 `cuckoo server start` 或帮用户以后台方式拉起。
2. **鉴权**：即使只监听 loopback 地址，也不代表"本机任何进程都可信"——应用数据目录下生成一个 token 文件（如 `~/Library/Application Support/Cuckoo/server.token`），所有 REST 请求和 SSE 订阅都必须携带标准 `Authorization: Bearer <token>` 请求头（这正是选择 HTTP 而非 WebSocket 的收益之一，见 3.12 节：不需要为"连接建立后再发一条鉴权消息"这种变通方案），未鉴权请求直接返回 401。桌面 UI 场景下，由于页面是通过 `tauri://` 加载的（而非普通网页），可以用一个极薄的 Tauri command（如 `get_server_token()`）让前端在启动时主动拉取 token，比“URL 参数/页面注入”这类针对普通网页的变通方案更自然且安全（token 不会出现在 URL 或页面 HTML 里）；前端 JS 拿到 token 后在后续所有 `fetch`/`EventSource` 请求里携带；CLI/MCP 启动时直接读取 token 文件。由于前端页面仅通过 `tauri://` 加载、不存在其他途径，也就不存在“非桌面场景的页面鉴权”这个问题。
3. **DNS 重绑定防护**：参考 MCP Streamable HTTP 安全建议，即使只监听 `127.0.0.1`，服务器仍需校验请求的 `Origin` 头以防 DNS 重绑定攻击（恶意网页诱导浏览器向 `127.0.0.1` 发请求），避免代理证书私钥、Collection 中可能存储的密钥信息被窃取。
4. **桌面场景的跨源请求（CORS）**：由于桌面 UI 的页面走 `tauri://` 加载、业务 API 请求发往 `http://127.0.0.1:<port>`，两者是不同源（浏览器同源策略视角），`cuckoo-server` 需要在 CORS 中间件里显式放行来自 `tauri://` 源的请求（Tauri 2.x 下 `WebviewWindow` 发起请求的 `Origin` 头形如 `tauri://localhost`）。这与第 3 点的 Origin 校验（防 DNS 重绑定）是同一个中间件的两个校验目的，实现时合并处理即可：维护一份"允许的 Origin 列表"（`tauri://localhost` + 各种形式的本机 loopback 地址），列表外的 Origin 一律拒绝。`cuckoo-server` 不面向局域网/公网开放，不存在"允许局域网访问"类的可选开关或配对码机制。

---

## 4. 前端架构调研

### 4.1 框架选择

Tauri v2 官方立场是完全前端无关，Vite 是 SPA 场景推荐工具链（不支持 SSR，因为没有常驻 Node 服务器）。React/Vue/Svelte/Solid 均为一等公民。Yaak（React 19 + Jotai）和 Hoppscotch（Vue 3）分别证明了两条路线都能撑起这个复杂度的应用。

**建议：React 19 + TypeScript + Vite**。理由：
- React 生态里 TanStack 系列（Query/Virtual/Table/Router）对"大型虚拟化数据表格 + 复杂状态管理"这类 DevTools 类应用的场景覆盖最深，直接命中我们最核心的"流量列表"需求；
- 与 Yaak（最直接的同类 Tauri 竞品）技术栈一致，出问题时可以直接参考其真实源码排查。

### 4.2 状态管理

不用单一 Redux 大 store（Bruno 的教训是 store 越来越臃肿，~200 个 action creator）。采用 **Jotai**（原子化状态，参考 Yaak）+ **TanStack Query**（管理与 Rust 后端的异步数据交互，自带缓存/失效机制）。**高频流量捕获流单独处理**：不走全局 store 触发整树重渲染，而是用事件订阅 + 按行局部更新（`React.memo` 按 flow id + `updatedAt` 版本号做浅比较）。

### 4.3 通用 UI 组件库：shadcn/ui（Tailwind CSS + Base UI）

**结论：Tailwind CSS + shadcn/ui，底层交互组件用 Base UI（而非 Radix UI）**。

shadcn/ui 不是传统意义上的 npm 包依赖，而是一个 CLI 工具，把组件源码直接拷贝进项目（`components/ui/`），样式基于 Tailwind、交互与无障碍访问逻辑委托给一个 headless 组件库。它的价值在于：起步阶段就有一套现成、专业感强的默认样式，同时代码完全落在自己仓库里，不受第三方库版本节奏和 API 边界的限制，可以随意深度定制。

底层 headless 组件库选 **Base UI** 而非 Radix UI：Base UI 是 Radix 原班人马联合 MUI 团队开发的下一代无样式组件库，定位是 Radix 的官方继任者，吸收了 Radix 多年实践的经验教训做了架构上的重新设计；shadcn/ui 官方已支持以 Base UI 作为底层生成组件。选择 Base UI 而非停留在 Radix 上，是面向长期维护路径的决定。

组件更新策略：`components/ui/` 目录视为"生成后基本不做侵入式修改"的区域，自定义的业务逻辑在其之上另外组合、不直接改动生成的源文件；样式层面的调整优先通过 Tailwind 主题变量（CSS variables）覆盖而非改动组件内部实现，这样官方后续的样式/能力更新可以用 `npx shadcn@latest diff <component>` 查看差异后按需挑选合并，不会因为改乱了源码而产生大范围冲突。真正需要走常规版本升级流程、可能包含安全修复的部分是 `@base-ui-components/react` 等 npm 依赖本身，这部分与普通库更新一样处理，不受"源码被拷贝"影响。

搭配的场景化库：`lucide-react`（图标）、`cmdk`（命令面板/快捷操作）、`sonner`（Toast 通知）、`react-hook-form` + `zod`（表单与校验，环境变量配置等场景）。

### 4.4 表格与树形组件

shadcn/ui（及其底层 Base UI）覆盖的是 Dialog/Dropdown/ContextMenu/Tooltip 这类通用交互组件，**不覆盖数据密集型的表格和树形结构**，这两类组件需要单独选型：

**表格（抓包流量列表）**：**TanStack Table**（headless 逻辑层）+ **`@tanstack/virtual`**（虚拟化）。两者同一作者维护，官方文档本身就有虚拟化表格的组合示例，风格与用法一致；不带任何默认样式，可以直接套用 shadcn 的视觉语言。右键菜单等交互通过在行元素上叠加 Base UI 的 `ContextMenu` 实现，与 TanStack Table 的数据层完全独立、互不干扰。高频更新场景（抓包过程中列表持续追加数据）需要注意 `columns` 引用稳定、`data` 增量更新避免整体替换，以及动态行高测量与列宽调整之间的联动。

**树形结构（Collection 树）**：**react-arborist**，内置虚拟化、拖拽排序、多选、键盘导航，渲染内容通过 render prop 自定义。它自带 `react-dnd` 作为拖拽引擎；如果后续其他拖拽场景统一使用 `@dnd-kit`，需要接受两套拖拽引擎并存，或改用更纯粹的 headless 方案（如 `@headless-tree/core`）自行接入 `@dnd-kit`，二者取舍取决于项目内拖拽场景的统一程度。

若后续出现"树形分组的表格"混合视图需求（如按域名分组折叠的流量列表），**AG Grid** 的 Tree Data 模式可以一次覆盖表格+树两种形态，代价是自成一套视觉体系、部分高级特性付费，视具体需求再评估是否引入。

### 4.5 布局：自研可调整大小面板（不引入 `react-resizable-panels`）

Yaak、Bruno、Hoppscotch 三个参考项目都不使用 dockview/golden-layout 级别的重量级可停靠多面板布局系统——Yaak 和 Bruno 是纯 CSS Grid/Flexbox 手写布局，Hoppscotch 用的是功能简单的 Vue 分栏组件 `splitpanes`（仅支持可拖拽调整比例，不支持标签页拖拽重排、浮动面板、布局持久化等高级能力）。这说明这个品类的实际产品形态是"请求列表 + 详情面板 + 可选侧边栏"这种相对固定的几块区域、比例可调，而非 VSCode 那种任意拖拽停靠。

**结论：不引入 `react-resizable-panels` 或任何第三方布局库，自行实现一个轻量的可调整大小面板组件**。需求边界很窄且明确——固定几块区域（水平/垂直切分）、拖拽 divider 调整比例、支持嵌套、比例持久化到 `localStorage`/后端设置——用一个 `<ResizablePanelGroup>`/`<ResizablePanel>`/`<ResizableHandle>` 风格的自研组件（内部用鼠标事件+`clamp`+CSS Grid `fr` 单位实现拖拽，双击 handle 重置默认比例，键盘可访问性通过 `role="separator"` + 方向键微调）即可完全覆盖，代码量在几百行量级，没有必要为此引入并长期跟随一个外部依赖的版本节奏。如果后续出现"独立 Tab 查看某个 Flow 详情"这类更高阶需求，优先在同一个自研组件基础上扩展，而非引入面向通用 IDE 场景设计的重量级停靠布局库（dockview/golden-layout 级别）。

### 4.6 虚拟化列表

流量捕获列表可能有数千行，必须虚拟化。推荐 **`@tanstack/virtual`**（框架无关、headless、支持动态行高测量、支持网格虚拟化），优于 `react-window`（更新频率较低、动态高度支持较弱）。搭配环形缓冲/上限淘汰策略（避免捕获数组无限增长）和批量节流更新（见 3.12 节）。

### 4.7 Flow / Transaction 数据模型——参考 Chrome DevTools Protocol

CDP 的 `Network` domain 是最成熟的同类数据模型参考，核心字段：

- **Request**: `url`, `method`, `headers`（保序，非普通 map，需支持重复/顺序以还原线路真实字节）, `postData`（大 body 惰性加载）
- **Response**: `status`, `headers`, `headersText`（原始未解析头块）, `remoteIPAddress`/`remotePort`, `protocol`（h2/http1.1/h3）, `securityDetails`（TLS 版本/密码套件/证书链）
- **ResourceTiming**（waterfall 各阶段耗时，均为相对 `requestTime` 的毫秒数）：`dnsStart/End`、`connectStart/End`、`sslStart/End`、`sendStart/End`、`receiveHeadersStart/End`（-1 表示该阶段不适用，如连接复用时无需 DNS）
- **生命周期事件**：`requestWillBeSent` → `responseReceived` → `dataReceived`（可能触发多次）→ `loadingFinished`/`loadingFailed`；body 是**按需拉取**而非随事件推送（`getResponseBody(requestId)`）——这个"元数据实时推送、body 惰性拉取"的模式我们必须照搬，否则大文件/视频流量会直接拖垮 SSE 推送通道。
- **WebSocket**: `webSocketCreated` → `webSocketWillSendHandshakeRequest`/`HandshakeResponseReceived`（握手视为普通请求/响应）→ 逐帧 `webSocketFrameSent`/`webSocketFrameReceived`（`{opcode, mask, payloadData}`）→ `webSocketClosed`。

我们自己的 `Flow` 类型设计直接沿用这套字段思路（详见 `spec.md` 中的数据模型章节）。

---

## 5. 综合结论：技术选型速查表

| 能力域 | 选型 | 备注 |
|---|---|---|
| 桌面壳 | **Tauri 2.x**，仅承载窗口/打包/极少数系统级能力 | 业务通信不走 Tauri IPC，见 3.12 节 |
| 通信协议 | **统一的 HTTP（请求-响应）+ SSE（服务端推送）**，桌面 UI / CLI / MCP 共用同一协议、同一 Server，不使用 WebSocket | 参考 OpenCode/Claude Code/MCP 官方 Streamable HTTP 的一致实践，见 3.12 节 |
| Rust 核心分层 | **Tauri-free core + Service 层 + 单一 Server（`cuckoo-server`）+ 极薄客户端**（CLI/MCP 都是该 Server 的 HTTP/SSE 客户端，桌面 UI 也是） | 比多入口各自适配的方案更彻底，见 3.12/3.13 节 |
| 少写胶水代码 | **方法签名唯一真源 + 宏/构建脚本生成 REST 路由表、SSE 事件类型、TS 客户端、CLI 通用调用、MCP tool schema** | 新增能力只改一处，见 3.13 节 |
| 本地 Server 生命周期与鉴权 | 内嵌启动或独立 `--headless` 运行；token 文件鉴权（桌面 UI 经 Tauri command 拉取，CLI/MCP 直接读文件）；不对局域网/公网开放 | 见 3.14 节 |
| API 客户端 HTTP 引擎 | **reqwest**（+ 需要精细计时时用 hyper 直接打点） | |
| gRPC（可选/二期） | `tonic` + `prost-reflect` | 参考 Yaak |
| MITM 代理核心 | **完全自研**（不用 hudsucker 等任何第三方整体封装库，自建 TCP accept/协议探测/CONNECT 隧道/Flow 持久化/断点拦截/系统集成） | 覆盖 h1/h2/WebSocket，底层复用 rustls/rcgen/h2/tokio-tungstenite |
| TLS | **rustls**（自己驱动 `LazyConfigAcceptor` 做 SNI 探测+动态证书） | 不用 native-tls/openssl 做代理内核 |
| 证书生成 | **rcgen**（自己写签发策略/缓存/CA 生命周期管理） | |
| HTTP/3 | 客户端功能用 `quinn`+`h3`（Beta），**MITM 拦截 v1 不做** | 明确排除范围 |
| WebSocket | `tokio-tungstenite`（自己驱动编解码+自研转发/拦截逻辑） | |
| GraphQL | 无需专门协议栈，前端展示层解析 | |
| 数据存储 | **SQLite + SeaORM**（`sea-orm` + `sea-orm-migration`，`sqlx-sqlite` 驱动 + `runtime-tokio-rustls`） | 参考 Yaak 用 SQLite 而非 Bruno 的纯文件方案，但 ORM 层选完整异步 `sea-orm` 而非 Yaak 的 `rusqlite`+`sea-query` 手搓方案，理由见 3.5 节；后续可加可选的文本导出/Git 同步 |
| 类型共享 | **ts-rs**（Rust struct → 自动生成 TS） | 避免 Bruno 踩过的双写漂移问题 |
| 前后端通信 | **统一的 HTTP + SSE**，批量聚合 + body 惰性拉取 | 桌面 UI 与 CLI/MCP/手机走同一协议、同一 Server，见 3.12 节 |
| 前端框架 | **React 19 + TypeScript + Vite** | |
| 状态管理 | **Jotai** + TanStack Query | 不用单体 Redux |
| UI 组件库 | **Tailwind CSS + shadcn/ui**，底层用 **Base UI**（非 Radix UI） | Base UI 是 Radix 原班人马+MUI 团队打造的继任者，shadcn 官方已支持；见 4.3 节 |
| 表格 | **TanStack Table** + `@tanstack/virtual` | headless，同一生态、无默认样式，与 shadcn 视觉风格兼容；见 4.4 节 |
| 树形结构 | **react-arborist** | 内置虚拟化/拖拽/多选/键盘导航，自带 `react-dnd`；见 4.4 节 |
| 布局 | **自研可调整大小面板组件**（固定几块区域+比例可调+持久化，不引入第三方布局库） | 三个参考项目均未用 dockview 级别的重量级停靠布局库，需求边界窄，自研成本低，见 4.5 节 |
| 虚拟列表 | **`@tanstack/virtual`** | |
| 系统代理配置/CA 安装 | 手写分平台胶水代码，参考 httptoolkit-server 分层 | 无成熟 crate 可用 |
| 按进程过滤流量 | **MVP：连接四元组反查本机 PID**（`GetExtendedTcpTable`/`/proc/net/tcp`+fd/`libproc`），仅作展示筛选元数据 | 内核级按 App 分流（NetworkExtension/WFP/eBPF）列为远期方向，见 3.10.1 节；Android 可做（`getConnectionOwnerUid`），iOS 不可做——已被 Reqable 官方文档证实，见 0.1.3 节 |
| 流量接入方式性能 | **系统代理 > TUN/WireGuard**（后者需在用户态重新跑一遍 TCP/IP 协议栈） | TUN/WireGuard 价值在免配置接入而非性能，见 3.10.2 节 |
| 产品形态对标 | **Reqable**（闭源，"Fiddler + Charles + Postman"） | 黑盒调研见 0.1 节；轻量化（体积/内存/启动时间）是核心卖点，支撑我们选 Tauri 而非 Electron |
| Flow 记录开关 | 代理转发引擎与"记录到列表/持久化"应可独立开关（参考 Reqable Turbo Mode） | 列为 MVP 后增强功能，见 0.1.2 节 |
| 脚本引擎 | MVP 维持 `rquickjs` 嵌入式沙箱，中期可选补充外部 Python3 解释器模式 | Reqable 用真 Python3 子进程，权衡对比见 0.1.4 节 |
| 功能缺口 | Diff（请求/响应对比）、Charles `.chls` 会话兼容、Access Control（代理访问控制） | 对标 Reqable 功能清单，需补充进 `spec.md`，见 0.1.2 节 |

此调研结论将作为 `spec.md`（产品规格与架构设计）与 `plan.md`（分阶段实施计划）的直接依据。
