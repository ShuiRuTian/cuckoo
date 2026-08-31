//! `cuckoo-server`：本地 HTTP+SSE Server（`axum`）——唯一的业务协议入口
//! （`spec.md` 2.1/2.2 节）。
//!
//! 既可以由 `cuckoo-desktop` 在同进程内 `tokio::spawn` 内嵌拉起（本文件对外
//! 导出的 [`spawn_server`]），也可以独立编译运行（`cuckoo-server --headless`，
//! 见 `main.rs`），两种方式内部持有的都是同一个 Service 层实例，行为完全一致。
//!
//! `cuckoo-server` 只做业务 API，不承担任何静态文件/前端页面的托管职责——
//! 桌面 UI 的页面统一由 Tauri 经 `tauri://` 协议加载，与本 Server 完全无关
//! （`spec.md` 2.2 节第 3 点）。

pub mod auth;
pub mod routes;
pub mod sse;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use auth::AuthState;

/// 内嵌启动后返回给调用方（`cuckoo-desktop`）的句柄信息。
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub token: String,
    pub join_handle: JoinHandle<()>,
    /// 代理状态：退出时用于停代理/恢复系统代理/取消挂起断点
    proxy_state: Arc<cuckoo_service::proxy_service::ProxyState>,
    /// Flow 聚合器：退出时 flush 残余事件
    flow_aggregator: Arc<cuckoo_flow::FlowAggregator>,
}

impl ServerHandle {
    /// 应用退出前的清理：停代理监听、取消挂起断点、恢复系统代理设置、
    /// 关闭事件聚合器。
    ///
    /// 不做会破坏幂等性或误伤用户配置的操作（只清除由本应用设置的系统代理）。
    pub async fn shutdown(&self) {
        let _ = cuckoo_service::proxy_service::stop_proxy(&self.proxy_state).await;
        self.flow_aggregator.shutdown().await;
        tracing::info!("cuckoo-server shutdown cleanup finished");
    }
}

/// 拼装完整的 `axum::Router`：业务路由 + SSE 端点 + 鉴权/Origin/CORS 中间件。
pub fn build_app(auth_state: AuthState) -> Router {
    // 打印 `#[rpc_method]` 登记的方法清单，便于启动期人工核对路由是否遗漏
    // （M0 阶段路由仍是手写 `.merge()`，这份清单先作为自检工具使用，
    // 见 `cuckoo_core::rpc_registry` 与 `routes/mod.rs` 的注释）。
    for descriptor in cuckoo_core::rpc_registry::all_descriptors() {
        tracing::info!(
            method = descriptor.method,
            path = descriptor.path,
            fn_name = descriptor.fn_name,
            "registered rpc method"
        );
    }

    let auth_routes = routes::api_router()
        .route("/api/flows/stream", get(sse::flow_stream))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::require_auth,
        ))
        .with_state(auth_state);

    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .merge(auth_routes)
        .layer(auth::cors_layer())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// 供 `cuckoo-desktop` 调用：在同进程内 `tokio::spawn` 拉起 `cuckoo-server`，
/// 监听 `127.0.0.1` 的一个操作系统分配的空闲端口（也可以传入固定端口）。
pub async fn spawn_server(port: Option<u16>) -> anyhow::Result<ServerHandle> {
    let token = auth::load_or_create_token()?;

    // 初始化数据库连接
    let db_path = cuckoo_store::default_db_path();
    let db = cuckoo_store::connect(db_path.to_str().unwrap_or("cuckoo.db")).await?;
    let db = Arc::new(db);

    // 初始化 CA 证书管理器
    let ca = cuckoo_ca::CaAuthority::load_or_create().await?;
    let ca = Arc::new(ca);

    // 初始化 Flow 事件聚合器和存储
    let flow_aggregator = cuckoo_flow::FlowAggregator::new();
    let flow_store = cuckoo_flow::FlowStore::new();

    // 初始化系统代理管理器
    let sys_proxy: Arc<dyn cuckoo_platform::SystemProxyManager> =
        Arc::from(cuckoo_platform::create_proxy_manager());

    // 初始化代理状态
    let proxy_state = Arc::new(cuckoo_service::proxy_service::ProxyState::new(
        flow_aggregator.clone(),
        flow_store.clone(),
        ca.clone(),
        sys_proxy,
    ));

    // 初始化规则状态（M3：与 proxy_state 共享同一 rule_engine 和 intercept_registry）
    let rule_state = Arc::new(cuckoo_service::rule_service::RuleState::from_proxy_state(
        proxy_state.rule_engine.clone(),
        proxy_state.intercept_registry.clone(),
    ));

    let auth_state = AuthState {
        token: Arc::new(token.clone()),
        db,
        ca,
        flow_aggregator: flow_aggregator.clone(),
        flow_store,
        proxy_state: proxy_state.clone(),
        rule_state,
    };

    let app = build_app(auth_state);

    let bind_addr: SocketAddr = format!("127.0.0.1:{}", port.unwrap_or(0)).parse()?;
    let listener = TcpListener::bind(bind_addr).await?;
    let addr = listener.local_addr()?;

    // 将端口写入文件，供 cuckoo-cli 探测连接
    let port_file = auth::token_file_path()
        .parent()
        .map(|p| p.join("server.port"))
        .unwrap_or_else(|| std::path::PathBuf::from("server.port"));
    let _ = std::fs::write(&port_file, addr.port().to_string());

    tracing::info!(%addr, "cuckoo-server listening; port file: {:?}", port_file);

    let join_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(?e, "cuckoo-server exited with error");
        }
    });

    Ok(ServerHandle {
        addr,
        token,
        join_handle,
        proxy_state,
        flow_aggregator,
    })
}
