# cuckoo-proxy

> MITM 代理内核（完全自研，不依赖第三方整体封装的代理库）。

## 功能（M2 阶段实现）

- TCP accept 循环 + 每连接 spawn task
- CONNECT 隧道解析与建立
- 协议探测（TLS ClientHello / 明文 HTTP / 未知协议兜底透传）
- TLS 动态签发与终止（接入 `cuckoo-ca`）
- HTTP/1.1 报文状态机（request-line / header / chunked 或 Content-Length body）
- 请求转发到真实上游服务器
- `ProxyHandler` trait（`on_request` / `on_response` / `should_intercept_tls`）
- M3：`RuleEngine`（Block / MapLocal / MapRemote / Rewrite / Breakpoint）
- M4：HTTP Upgrade 识别 + WebSocket 帧编解码

## 当前状态

M2 阶段已实现：TCP accept 循环、CONNECT 隧道、TLS 动态签发、HTTP/1.1 状态机、请求转发、ProxyHandler trait。
`sniff.rs`（协议探测）和 `http2.rs` 留到后续阶段。

## 目录结构

```
src/
├── lib.rs           # 模块入口，重导出 start_proxy / ProxyServer / ProxyHandler 等
├── listener.rs      # TcpListener accept 循环 + start_proxy() + ProxyServer
├── connect.rs       # CONNECT 请求解析与隧道建立
├── tls.rs           # TLS 动态签发与终止（接入 cuckoo-ca）
├── http1.rs         # HTTP/1.1 报文状态机（request-line / header / chunked / Content-Length）
├── forward.rs       # 请求转发到上游服务器
├── handler.rs       # ProxyHandler trait + CuckooProxyHandler + FlowEmittingHandler + RequestAction/ResponseAction
├── error.rs         # ProxyError / ProxyResult 错误类型
│  (待实现)
├── sniff.rs         # 🔲 协议探测（M2 末尾）
├── http2.rs         # 🔲 HTTP/2 帧级状态机（M2 末尾或 M5）
├── ws.rs            # 🔲 WebSocket 帧编解码（M4）
├── rule_engine.rs   # 🔲 拦截规则匹配引擎（M3）
└── intercept.rs     # 🔲 断点拦截机制（M3）
```

## 依赖关系

- 将被 `cuckoo-service` 依赖（`proxy_service::start_proxy / stop_proxy`）
- 依赖 `cuckoo-ca`（证书签发）、`cuckoo-flow`（Flow 数据模型）、`cuckoo-http`（连接复用）
- **全项目技术风险最集中的 crate**，见 `plan.md` 风险提醒
