//! `proxy_service`：代理启停与状态查询（`spec.md` 7.2 节，`plan.md` M2.3/M2.4 节）。
//!
//! 当前实现：
//! - `start_proxy()`：启动 MITM 代理监听（防重复启动）
//! - `stop_proxy()`：停止代理（取消挂起断点、恢复系统代理）
//! - `get_proxy_status()`：查询代理状态
//!
//! Flow 查询端点（`GET /api/flows` 等）在 `cuckoo-server/src/routes/flow.rs`
//! 直接读取 `AuthState.flow_store` 实现，不经过本模块。
//!
//! 代理内核（`cuckoo-proxy`）在 M2.2 已实现，这里将其接入 Service 层。

use std::sync::Arc;

use cuckoo_core::{ServiceError, ServiceResult};
use cuckoo_dto::StartProxyInput;
use cuckoo_flow::{FlowAggregator, FlowStore};
use cuckoo_macros::rpc_method;
use cuckoo_platform::SystemProxyManager;

/// 代理服务状态（运行时持有）。
pub struct ProxyState {
    /// Flow 事件聚合器（SSE 端点订阅）
    pub flow_aggregator: Arc<FlowAggregator>,
    /// Flow 环形缓冲存储
    pub flow_store: FlowStore,
    /// 代理服务句柄（启动后填充）
    pub proxy_server: tokio::sync::Mutex<Option<cuckoo_proxy::ProxyServer>>,
    /// CA 证书管理器
    pub ca: Arc<cuckoo_ca::CaAuthority>,
    /// 系统代理管理器
    pub sys_proxy: Arc<dyn SystemProxyManager>,
    /// 规则引擎（M3 新增）
    pub rule_engine: Arc<cuckoo_proxy::RuleEngine>,
    /// 断点注册表（M3 新增）
    pub intercept_registry: Arc<cuckoo_proxy::InterceptRegistry>,
    /// 系统代理是否由本应用设置。
    ///
    /// 仅在为 true 时才在停止/退出时清除系统代理，
    /// 避免把用户自己配置的代理设置抹掉。
    system_proxy_set_by_us: std::sync::atomic::AtomicBool,
}

impl ProxyState {
    pub fn new(
        flow_aggregator: Arc<FlowAggregator>,
        flow_store: FlowStore,
        ca: Arc<cuckoo_ca::CaAuthority>,
        sys_proxy: Arc<dyn SystemProxyManager>,
    ) -> Self {
        Self {
            flow_aggregator,
            flow_store,
            ca,
            sys_proxy,
            proxy_server: tokio::sync::Mutex::new(None),
            rule_engine: Arc::new(cuckoo_proxy::RuleEngine::new()),
            intercept_registry: Arc::new(cuckoo_proxy::InterceptRegistry::new()),
            system_proxy_set_by_us: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 记录系统代理已由本应用设置（仅在 set_proxy 成功后调用）。
    pub fn mark_system_proxy_set(&self) {
        self.system_proxy_set_by_us
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// 若系统代理由本应用设置，则清除并返回 true；否则返回 false
    /// （不碰用户自己配置的代理）。
    pub fn clear_system_proxy_if_ours(&self) -> bool {
        if self
            .system_proxy_set_by_us
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            if let Err(e) = self.sys_proxy.clear_proxy() {
                tracing::warn!(?e, "failed to clear system proxy on shutdown");
            }
            true
        } else {
            false
        }
    }
}

/// 代理状态响应 DTO。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct ProxyStatus {
    pub running: bool,
    pub listen_addr: Option<String>,
    pub port: Option<u16>,
    pub flow_count: usize,
}

/// 启动代理监听。
///
/// 标注为 `POST /api/proxy/start`（`spec.md` 7.2 节），请求体
/// 为 `{ "port": 8080 }`（port 缺省/0 时由操作系统分配空闲端口）。
#[rpc_method("POST", "/api/proxy/start")]
pub async fn start_proxy(
    input: StartProxyInput,
    state: &ProxyState,
) -> ServiceResult<ProxyStatus> {
    let port = input.port.unwrap_or(0);

    let mut guard = state.proxy_server.lock().await;

    // 防重复启动：旧监听器还往里塞会泄漏（drop JoinHandle 不会 abort
    // tokio task，旧 accept 循环继续占着旧端口）。要求先显式 stop。
    if guard.is_some() {
        return Err(ServiceError::BadRequest(
            "proxy already running, stop it first".to_string(),
        ));
    }

    let handler = Arc::new(cuckoo_proxy::FlowEmittingHandler::new(
        state.flow_aggregator.clone(),
        state.flow_store.clone(),
        state.rule_engine.clone(),
        state.intercept_registry.clone(),
    )) as cuckoo_proxy::SharedHandler;
    let ca = state.ca.clone();

    let server = cuckoo_proxy::start_proxy(port, handler, ca)
        .await
        .map_err(|e| ServiceError::Internal(format!("failed to start proxy: {e}")))?;

    let status = ProxyStatus {
        running: true,
        listen_addr: Some(server.listen_addr.to_string()),
        port: Some(server.listen_addr.port()),
        flow_count: state.flow_store.len(),
    };

    *guard = Some(server);
    drop(guard); // 先释放锁再操作系统代理，避免长时间持锁

    // 自动设置系统代理
    if let Err(e) = state.sys_proxy.set_proxy("127.0.0.1", status.port.unwrap_or(port)) {
        tracing::warn!(?e, "failed to set system proxy (proxy is running but system proxy not configured)");
    } else {
        // 记录由本应用设置，停止/退出时才负责恢复
        state.mark_system_proxy_set();
    }

    Ok(status)
}

/// 停止代理监听。
///
/// 标注为 `POST /api/proxy/stop`。
#[rpc_method("POST", "/api/proxy/stop")]
pub async fn stop_proxy(state: &ProxyState) -> ServiceResult<ProxyStatus> {
    {
        let mut guard = state.proxy_server.lock().await;
        if let Some(server) = guard.take() {
            // abort accept 循环（已 spawn 的连接 task 会随连接结束自行退出）
            server.join_handle.abort();
        }
    }

    // 取消所有挂起中的断点：否则挂起协程永远等待，
    // 对应的客户端连接永久悬挂
    state.intercept_registry.cancel_all();

    // 仅清除由本应用设置的系统代理（残留指向已死端口会导致整机
    // HTTP 流量黑洞；但用户自己配的代理不能动）
    state.clear_system_proxy_if_ours();

    Ok(ProxyStatus {
        running: false,
        listen_addr: None,
        port: None,
        flow_count: state.flow_store.len(),
    })
}

/// 查询代理状态。
///
/// 标注为 `GET /api/proxy/status`。
#[rpc_method("GET", "/api/proxy/status")]
pub async fn get_proxy_status(state: &ProxyState) -> ServiceResult<ProxyStatus> {
    let guard = state.proxy_server.lock().await;
    let running = guard.is_some();
    let listen_addr = guard.as_ref().map(|s| s.listen_addr.to_string());
    let port = guard.as_ref().map(|s| s.listen_addr.port());

    Ok(ProxyStatus {
        running,
        listen_addr,
        port,
        flow_count: state.flow_store.len(),
    })
}
