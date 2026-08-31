//! System 路由：CA 证书导出。
//!
//! 对应 `cuckoo_service::system_service::export_ca_cert`（`plan.md` M2.1 节）。
//! 从 `AuthState.ca` 获取 `CaAuthority`，调用 Service 层方法返回 `CaCertInfo`。

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_dto::CaCertInfo;
use cuckoo_service::system_service;

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new().route("/api/certs/export", post(handle_export_ca_cert))
}

async fn handle_export_ca_cert(
    State(state): State<AuthState>,
) -> Result<Json<CaCertInfo>, ServiceError> {
    let info = system_service::export_ca_cert(&state.ca).await?;
    Ok(Json(info))
}
