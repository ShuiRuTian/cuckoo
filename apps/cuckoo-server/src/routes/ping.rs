//! `GET /api/ping` —— 对应 `cuckoo_service::ping()`（M0 端到端闭环验证端点）。

use axum::routing::get;
use axum::{Json, Router};
use cuckoo_dto::PongResponse;

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new().route("/api/ping", get(handle_ping))
}

async fn handle_ping() -> Json<PongResponse> {
    Json(cuckoo_service::ping().await)
}
