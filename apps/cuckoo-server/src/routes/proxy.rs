//! Proxy 路由：代理启停与状态查询。
//!
//! 对应 `cuckoo_service::proxy_service` 的 `start_proxy` / `stop_proxy` /
//! `get_proxy_status` 方法（`plan.md` M2.2/M2.4 节）。

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_dto::StartProxyInput;
use cuckoo_service::proxy_service::{ProxyStatus, start_proxy, stop_proxy, get_proxy_status};

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/api/proxy/start", post(handle_start_proxy))
        .route("/api/proxy/stop", post(handle_stop_proxy))
        .route("/api/proxy/status", get(handle_get_proxy_status))
}

async fn handle_start_proxy(
    State(state): State<AuthState>,
    axum::Json(input): axum::Json<StartProxyInput>,
) -> Result<Json<ProxyStatus>, ServiceError> {
    let status = start_proxy(input, &state.proxy_state).await?;
    Ok(Json(status))
}

async fn handle_stop_proxy(
    State(state): State<AuthState>,
) -> Result<Json<ProxyStatus>, ServiceError> {
    let status = stop_proxy(&state.proxy_state).await?;
    Ok(Json(status))
}

async fn handle_get_proxy_status(
    State(state): State<AuthState>,
) -> Result<Json<ProxyStatus>, ServiceError> {
    let status = get_proxy_status(&state.proxy_state).await?;
    Ok(Json(status))
}
