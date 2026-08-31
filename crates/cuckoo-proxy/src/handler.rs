//! `ProxyHandler` trait 定义（`spec.md` 4.2 节）。
//!
//! 统一的事件驱动接口：所有协议层（HTTP/1.1、HTTP/2、WebSocket）解析出的
//! 请求/响应都经过同一套 handler 流程（规则匹配 → 断点判断 → 转发/短路/丢弃）。
//!
//! M2 阶段提供两个实现：
//! - `CuckooProxyHandler`：只打日志的默认实现
//! - `FlowEmittingHandler`：接入 `FlowAggregator` + `FlowStore`，产生 `TrafficEvent`
//!
//! M3 阶段升级：
//! - `ProxyHandler` trait 改为 async（`async_trait`）
//! - `RequestAction` / `ResponseAction` 新增 `Pause(flow_id, stage)` 变体
//! - `FlowEmittingHandler` 接入 `RuleEngine` + `InterceptRegistry`

use std::sync::Arc;

use async_trait::async_trait;
use cuckoo_flow::{
    Flow, FlowAggregator, FlowProtocol, FlowStatus, FlowStore, FlowTiming,
    HttpMessage as FlowHttpMessage, InterceptState, SocketAddrInfo, TrafficEvent,
};
use ulid::Ulid;

use crate::error::ProxyResult;
use crate::intercept::{InterceptDecision, InterceptRegistry};
use crate::rule_engine::{RuleEngine, RuleOutcome};

/// 简化的 HTTP 消息表示（内部传递用，非最终 DTO）。
///
/// TS 绑定重命名为 `ProxyHttpMessage`，避免与 `cuckoo_flow::HttpMessage` 同名冲突。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/", rename = "ProxyHttpMessage")]
pub struct HttpMessage {
    pub method: String,
    pub uri: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpMessage {
    /// 获取第一个匹配 header 的值。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 从 URI 中提取 host:port（用于 CONNECT 目标或 Host header）。
    pub fn host(&self) -> Option<&str> {
        self.header("host")
            .or_else(|| self.uri.split("://").nth(1).and_then(|s| s.split('/').next()))
    }
}

/// 请求处理动作（`spec.md` 4.2 节）。
#[derive(Debug)]
pub enum RequestAction {
    /// 放行（可能已改写）继续转发到上游。
    Forward(HttpMessage),
    /// 短路：直接返回给客户端，不转发（Block/MapLocal 场景）。
    Respond(HttpMessage),
    /// 挂起，等待前端断点放行/修改/丢弃（M3 新增）。
    Pause(String, String), // (flow_id, stage)
}

/// 响应处理动作（`spec.md` 4.2 节）。
#[derive(Debug)]
pub enum ResponseAction {
    /// 放行（可能已改写）返回给客户端。
    Forward(HttpMessage),
    /// 挂起，等待前端断点放行/修改/丢弃（M3 新增）。
    Pause(String, String), // (flow_id, stage)
}

/// 连接上下文：携带目标地址、SNI 等信息供 handler 决策。
#[derive(Debug, Clone)]
pub struct FlowContext {
    /// 目标 host:port（从 CONNECT 或 Host header 解析）。
    pub target_host: String,
    pub target_port: u16,
    /// SNI（TLS 连接时才有）。
    pub sni: Option<String>,
    /// Flow ID（M3 新增：handler 创建后传递给 listener 用于关联 request/response）。
    pub flow_id: Option<String>,
}

impl FlowContext {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            target_host: host.to_string(),
            target_port: port,
            sni: None,
            flow_id: None,
        }
    }

    pub fn with_sni(mut self, sni: Option<String>) -> Self {
        self.sni = sni;
        self
    }

    pub fn with_flow_id(mut self, flow_id: String) -> Self {
        self.flow_id = Some(flow_id);
        self
    }
}

/// 代理处理器 trait（`spec.md` 4.2 节）。
///
/// M3 阶段改为 async_trait，以支持断点挂起（`pause_and_wait`）。
#[async_trait]
pub trait ProxyHandler: Send + Sync + 'static {
    /// 收到请求头后调用，可放行/改写/短路返回响应/挂起等待断点。
    ///
    /// `ctx` 为可变引用：handler 内部创建 Flow 后应回填 `ctx.flow_id`，
    /// 供后续 `on_response` 精确关联（并发请求下同一 host 的多条 Flow
    /// 无法靠 host 猜测区分）。
    async fn on_request(
        &self,
        ctx: &mut FlowContext,
        req: &HttpMessage,
    ) -> ProxyResult<RequestAction>;

    /// 收到响应后调用，可放行/改写/挂起等待断点。
    async fn on_response(
        &self,
        ctx: &FlowContext,
        res: &HttpMessage,
    ) -> ProxyResult<ResponseAction>;

    /// TLS ClientHello 到达时调用，决定是否做 MITM。
    /// 返回 `false` 则原样透传，不解密（兼容证书锁定场景）。
    fn should_intercept_tls(&self, sni: Option<&str>, ctx: &FlowContext) -> bool {
        let _ = (sni, ctx);
        true // 默认总是拦截
    }
}

/// 默认实现：只打日志，不做任何拦截或改写。
pub struct CuckooProxyHandler;

#[async_trait]
impl ProxyHandler for CuckooProxyHandler {
    async fn on_request(
        &self,
        ctx: &mut FlowContext,
        req: &HttpMessage,
    ) -> ProxyResult<RequestAction> {
        tracing::info!(
            method = %req.method,
            uri = %req.uri,
            host = %ctx.target_host,
            "proxy request"
        );
        Ok(RequestAction::Forward(req.clone()))
    }

    async fn on_response(
        &self,
        ctx: &FlowContext,
        res: &HttpMessage,
    ) -> ProxyResult<ResponseAction> {
        tracing::info!(
            status = ?res.version,
            host = %ctx.target_host,
            "proxy response"
        );
        Ok(ResponseAction::Forward(res.clone()))
    }
}

/// 共享 handler 的类型别名。
pub type SharedHandler = Arc<dyn ProxyHandler>;

// ────────────────────────────────────────────────────────────────────
// FlowEmittingHandler：接入 FlowAggregator + FlowStore + RuleEngine + InterceptRegistry
// ────────────────────────────────────────────────────────────────────

/// M3 代理消息 body 大小上限（超出则截断）。
const MAX_BODY_SIZE: usize = 256 * 1024; // 256 KiB

/// 接入 `FlowAggregator` + `FlowStore` + `RuleEngine` + `InterceptRegistry` 的 handler 实现。
///
/// M3 升级：
/// - `on_request`：先应用规则链（Block/MapLocal/MapRemote/Rewrite），再判断断点
/// - `on_response`：应用响应规则链 + 响应阶段断点
/// - 断点命中时调用 `InterceptRegistry::pause_and_wait` 挂起协程
pub struct FlowEmittingHandler {
    aggregator: Arc<FlowAggregator>,
    store: FlowStore,
    /// 规则引擎（M3 新增）
    rule_engine: Arc<RuleEngine>,
    /// 断点注册表（M3 新增）
    intercept_registry: Arc<InterceptRegistry>,
}

impl FlowEmittingHandler {
    pub fn new(
        aggregator: Arc<FlowAggregator>,
        store: FlowStore,
        rule_engine: Arc<RuleEngine>,
        intercept_registry: Arc<InterceptRegistry>,
    ) -> Self {
        Self {
            aggregator,
            store,
            rule_engine,
            intercept_registry,
        }
    }

    /// 将代理内部 `HttpMessage` 转换为 `cuckoo_flow::HttpMessage` DTO。
    fn to_flow_message(msg: &HttpMessage, is_response: bool) -> FlowHttpMessage {
        let (body, body_truncated) = if msg.body.len() > MAX_BODY_SIZE {
            (msg.body[..MAX_BODY_SIZE].to_vec(), true)
        } else {
            (msg.body.clone(), false)
        };

        // 响应：从 `:status` 伪 header（如 "403 Forbidden"）解析状态码与完整状态行。
        // 请求：start_line = "METHOD URI VERSION"。
        let (start_line, status_code) = if is_response {
            let status = msg
                .header(":status")
                .map(|s| s.to_string())
                .unwrap_or_else(|| "200 OK".to_string());
            let code = status
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<u16>().ok());
            (
                format!("{} {}", msg.version, status),
                code,
            )
        } else {
            (
                format!("{} {} {}", msg.method, msg.uri, msg.version),
                None,
            )
        };

        FlowHttpMessage {
            start_line,
            method: if is_response { String::new() } else { msg.method.clone() },
            uri: if is_response { String::new() } else { msg.uri.clone() },
            version: msg.version.clone(),
            status_code,
            headers: msg.headers.clone(),
            headers_raw: None,
            body,
            body_size: msg.body.len(),
            body_truncated,
        }
    }

    /// 创建新的 Flow 记录（收到请求时）。
    fn create_flow(ctx: &FlowContext, req: &HttpMessage) -> Flow {
        let now = chrono::Utc::now().timestamp_millis();
        let id = Ulid::new().to_string();

        let client_addr = SocketAddrInfo {
            ip: "127.0.0.1".to_string(),
            port: 0,
        };

        let server_addr = SocketAddrInfo {
            ip: ctx.target_host.clone(),
            port: ctx.target_port,
        };

        Flow {
            id,
            protocol: FlowProtocol::Http1,
            client_addr: Some(client_addr),
            server_addr: Some(server_addr),
            request: Self::to_flow_message(req, false),
            response: None,
            timing: FlowTiming {
                start_time: now,
                send_start: Some(now),
                ..Default::default()
            },
            tls: ctx.sni.as_ref().map(|sni| cuckoo_flow::TlsInfo {
                version: "TLS 1.2+".to_string(),
                cipher: String::new(),
                sni: Some(sni.clone()),
                alpn: Some("http/1.1".to_string()),
            }),
            status: FlowStatus::Pending,
            error: None,
            intercept: InterceptState::NotIntercepted,
            tags: Vec::new(),
        }
    }

    /// 发送 TrafficEvent 到聚合器（非阻塞，channel 满或已关闭则丢弃）。
    fn emit_event(&self, event: TrafficEvent) {
        if let Some(tx) = self.aggregator.sender() {
            let _ = tx.try_send(event);
        }
    }

    /// 处理断点挂起：如果有断点规则命中，调用 `InterceptRegistry::pause_and_wait`。
    ///
    /// 仅由显式 Breakpoint 规则触发；Rewrite/MapRemote 是自动改写，不挂起。
    ///
    /// 返回最终的 `RequestAction`：
    /// - `Continue(edited)` → `Forward(edited)`
    /// - `Abort` / `DropConnection` → `Respond(空响应)`（上层会关闭连接）
    async fn maybe_breakpoint_request(
        &self,
        flow_id: &str,
        req: HttpMessage,
    ) -> ProxyResult<RequestAction> {
        // 更新 Flow 状态为 Intercepted
        if let Some(mut flow) = self.store.get(flow_id) {
            flow.status = FlowStatus::Intercepted;
            flow.intercept = InterceptState::Paused {
                stage: "request".to_string(),
            };
            self.store.upsert(flow.clone());
            self.emit_event(TrafficEvent::FlowIntercepted {
                flow_id: flow_id.to_string(),
                stage: "request".to_string(),
            });
        }

        // 挂起等待前端决策
        let decision = self
            .intercept_registry
            .pause_and_wait(flow_id, "request", req.clone())
            .await
            .map_err(|e| crate::error::ProxyError::Handler(format!("intercept error: {e}")))?;

        match decision {
            InterceptDecision::Continue { edited } => {
                // 前端可能修改了请求
                let final_req = edited.unwrap_or(req);

                // 更新 Flow 状态
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Pending;
                    flow.intercept = InterceptState::Resumed;
                    flow.request = Self::to_flow_message(&final_req, false);
                    self.store.upsert(flow);
                }

                Ok(RequestAction::Forward(final_req))
            }
            InterceptDecision::Abort => {
                // 返回空响应
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Complete;
                    flow.intercept = InterceptState::Resumed;
                    flow.response = Some(Self::to_flow_message(&build_abort_response(), true));
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow_id.to_string(),
                        flow,
                    });
                }
                Ok(RequestAction::Respond(build_abort_response()))
            }
            InterceptDecision::DropConnection => {
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Error;
                    flow.error = Some("connection dropped by user".to_string());
                    flow.intercept = InterceptState::Resumed;
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowError {
                        flow_id: flow.id.clone(),
                        error: "connection dropped by user".to_string(),
                    });
                }
                Ok(RequestAction::Respond(build_abort_response()))
            }
        }
    }

    /// 处理响应阶段的断点挂起。
    async fn maybe_breakpoint_response(
        &self,
        flow_id: &str,
        res: HttpMessage,
    ) -> ProxyResult<ResponseAction> {
        if let Some(mut flow) = self.store.get(flow_id) {
            flow.status = FlowStatus::Intercepted;
            flow.intercept = InterceptState::Paused {
                stage: "response".to_string(),
            };
            flow.response = Some(Self::to_flow_message(&res, true));
            self.store.upsert(flow.clone());
            self.emit_event(TrafficEvent::FlowIntercepted {
                flow_id: flow_id.to_string(),
                stage: "response".to_string(),
            });
        }

        let decision = self
            .intercept_registry
            .pause_and_wait(flow_id, "response", res.clone())
            .await
            .map_err(|e| crate::error::ProxyError::Handler(format!("intercept error: {e}")))?;

        match decision {
            InterceptDecision::Continue { edited } => {
                let final_res = edited.unwrap_or(res);
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Complete;
                    flow.intercept = InterceptState::Resumed;
                    flow.response = Some(Self::to_flow_message(&final_res, true));
                    let now = chrono::Utc::now().timestamp_millis();
                    flow.timing.ttfb = Some(now);
                    flow.timing.end_time = Some(now);
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow_id.to_string(),
                        flow,
                    });
                }
                Ok(ResponseAction::Forward(final_res))
            }
            InterceptDecision::Abort => {
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Complete;
                    flow.intercept = InterceptState::Resumed;
                    flow.response = Some(Self::to_flow_message(&build_abort_response(), true));
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow_id.to_string(),
                        flow,
                    });
                }
                Ok(ResponseAction::Forward(build_abort_response()))
            }
            InterceptDecision::DropConnection => {
                if let Some(mut flow) = self.store.get(flow_id) {
                    flow.status = FlowStatus::Error;
                    flow.error = Some("connection dropped by user".to_string());
                    flow.intercept = InterceptState::Resumed;
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowError {
                        flow_id: flow.id.clone(),
                        error: "connection dropped by user".to_string(),
                    });
                }
                Ok(ResponseAction::Forward(build_abort_response()))
            }
        }
    }
}

#[async_trait]
impl ProxyHandler for FlowEmittingHandler {
    async fn on_request(
        &self,
        ctx: &mut FlowContext,
        req: &HttpMessage,
    ) -> ProxyResult<RequestAction> {
        let flow = Self::create_flow(ctx, req);

        // 回填 flow_id，供 on_response 精确关联（避免并发请求下响应错配）
        ctx.flow_id = Some(flow.id.clone());

        tracing::info!(
            method = %req.method,
            uri = %req.uri,
            host = %ctx.target_host,
            flow_id = %flow.id,
            "proxy request"
        );

        // 存入 store 并发送 FlowStarted 事件
        self.store.upsert(flow.clone());
        self.emit_event(TrafficEvent::FlowStarted { flow: flow.clone() });

        // M3: 应用延迟规则（ThrottleOrDelay.delay_ms 总和）。
        // 放在规则链之前：延迟与短路/改写/断点正交，统一在此生效
        //（对标 Charles 的 Throttle 对整个生效周期加延迟）。
        let delay = self.rule_engine.compute_request_delay(req);
        if !delay.is_zero() {
            tracing::debug!(?delay, flow_id = %flow.id, "applying throttle delay");
            tokio::time::sleep(delay).await;
        }

        // M3: 应用规则链
        match self.rule_engine.apply_request_rules(req) {
            RuleOutcome::Unchanged => {
                // 无规则匹配，正常转发
                Ok(RequestAction::Forward(req.clone()))
            }
            RuleOutcome::ShortCircuit(resp) => {
                // Block / MapLocal：直接返回，Flow 进入终态
                if let Some(mut flow) = self.store.get(&flow.id) {
                    flow.status = FlowStatus::Complete;
                    flow.response = Some(Self::to_flow_message(&resp, true));
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow.id.clone(),
                        flow,
                    });
                }
                Ok(RequestAction::Respond(resp))
            }
            RuleOutcome::Rewritten(rewritten) => {
                // Rewrite / MapRemote：自动改写后直接放行（不挂起）。
                // 断点仅由显式 Breakpoint 规则触发。
                if let Some(mut flow) = self.store.get(&flow.id) {
                    flow.request = Self::to_flow_message(&rewritten, false);
                    self.store.upsert(flow);
                }
                Ok(RequestAction::Forward(rewritten))
            }
            RuleOutcome::Pause(_, _stage) => {
                // Breakpoint 规则命中：挂起等待前端决策
                self.maybe_breakpoint_request(&flow.id, req.clone()).await
            }
        }
    }

    async fn on_response(
        &self,
        ctx: &FlowContext,
        res: &HttpMessage,
    ) -> ProxyResult<ResponseAction> {
        tracing::info!(
            status_line = %res.version,
            host = %ctx.target_host,
            flow_id = ?ctx.flow_id,
            "proxy response"
        );

        let now = chrono::Utc::now().timestamp_millis();

        // 查找对应的 Flow：优先用 on_request 回填的 flow_id 精确匹配；
        // flow_id 缺失时（非 FlowEmittingHandler 创建的连接等）退化为
        // 按 host 在最近 Pending Flow 中猜匹配。
        let flow_opt = match ctx.flow_id.as_deref() {
            Some(id) => self.store.get(id).filter(|f| f.status == FlowStatus::Pending),
            None => self
                .store
                .query_recent(50, 0)
                .into_iter()
                .find(|f| {
                    f.status == FlowStatus::Pending
                        && f.server_addr.as_ref().map(|a| &a.ip) == Some(&ctx.target_host)
                }),
        };

        if let Some(mut flow) = flow_opt {
            // M3: 应用响应规则链
            let original_req_msg = flow.request.clone();
            let original_req = HttpMessage {
                method: original_req_msg.method,
                uri: original_req_msg.uri,
                version: original_req_msg.version,
                headers: original_req_msg.headers,
                body: original_req_msg.body,
            };

            match self.rule_engine.apply_response_rules(res, &original_req) {
                RuleOutcome::Unchanged => {
                    // 无规则匹配
                    flow.response = Some(Self::to_flow_message(res, true));
                    flow.status = FlowStatus::Complete;
                    flow.timing.ttfb = Some(now);
                    flow.timing.end_time = Some(now);
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow.id.clone(),
                        flow,
                    });
                    Ok(ResponseAction::Forward(res.clone()))
                }
                RuleOutcome::Rewritten(rewritten) => {
                    // 响应被 Rewrite 自动改写：直接放行（不挂起），Flow 进入终态
                    flow.response = Some(Self::to_flow_message(&rewritten, true));
                    flow.status = FlowStatus::Complete;
                    flow.timing.ttfb = Some(now);
                    flow.timing.end_time = Some(now);
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow.id.clone(),
                        flow,
                    });
                    Ok(ResponseAction::Forward(rewritten))
                }
                RuleOutcome::ShortCircuit(resp) => {
                    // 响应阶段的 ShortCircuit 不太常见，但支持
                    flow.response = Some(Self::to_flow_message(&resp, true));
                    flow.status = FlowStatus::Complete;
                    flow.timing.ttfb = Some(now);
                    flow.timing.end_time = Some(now);
                    self.store.upsert(flow.clone());
                    self.emit_event(TrafficEvent::FlowComplete {
                        flow_id: flow.id.clone(),
                        flow,
                    });
                    Ok(ResponseAction::Forward(resp))
                }
                RuleOutcome::Pause(_, _stage) => {
                    // 响应阶段断点
                    self.maybe_breakpoint_response(&flow.id, res.clone()).await
                }
            }
        } else {
            // 没有找到对应的 Flow，直接放行
            Ok(ResponseAction::Forward(res.clone()))
        }
    }
}

impl Clone for FlowEmittingHandler {
    fn clone(&self) -> Self {
        Self {
            aggregator: self.aggregator.clone(),
            store: self.store.clone(),
            rule_engine: self.rule_engine.clone(),
            intercept_registry: self.intercept_registry.clone(),
        }
    }
}

/// 构造一个 Abort 响应（403 Forbidden，空 body）。
fn build_abort_response() -> HttpMessage {
    HttpMessage {
        method: String::new(),
        uri: String::new(),
        version: "HTTP/1.1".to_string(),
        headers: vec![
            (":status".to_string(), "403 Forbidden".to_string()),
            ("Content-Length".to_string(), "0".to_string()),
        ],
        body: Vec::new(),
    }
}
