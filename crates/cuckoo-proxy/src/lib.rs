//! `cuckoo-proxy`：MITM 代理内核（完全自研，不依赖第三方整体封装的代理库）。
//!
//! 基于 `spec.md` 第 4 章的设计：
//! - `listener.rs`：TCP accept 循环，每个连接 spawn 一个 task
//! - `connect.rs`：CONNECT 请求解析与隧道建立
//! - `tls.rs`：TLS 动态签发与终止（接入 `cuckoo-ca`）
//! - `http1.rs`：自研 HTTP/1.1 报文状态机
//! - `forward.rs`：请求转发到上游服务器
//! - `handler.rs`：`ProxyHandler` trait 定义 + 默认实现
//!
//! M2 阶段最小可用版本：
//! - 支持 CONNECT 隧道 + TLS 终止 + HTTP/1.1 解析转发
//! - 不支持 keep-alive 连接复用、chunked encoding 的畸形分片
//! - 不支持 HTTP/2（ALPN 协商到 h2 时降级为 HTTP/1.1）

pub mod connect;
pub mod error;
pub mod forward;
pub mod handler;
pub mod http1;
pub mod intercept;
pub mod listener;
pub mod rule_engine;
pub mod tls;

pub use error::{ProxyError, ProxyResult};
pub use handler::{
    CuckooProxyHandler, FlowContext, FlowEmittingHandler, HttpMessage, ProxyHandler, RequestAction,
    ResponseAction, SharedHandler,
};
pub use intercept::{
    InterceptDecision, InterceptError, InterceptRegistry, SharedInterceptRegistry,
};
pub use listener::{start_proxy, ProxyServer};
pub use rule_engine::{
    InterceptRule, RuleEngine, RuleEntry, RuleMatcher, RuleOutcome, RewriteOp,
    SharedRuleEngine,
};
