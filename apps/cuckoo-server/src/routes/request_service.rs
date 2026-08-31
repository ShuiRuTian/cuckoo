//! Request 执行路由：`POST /api/requests/send`。
//!
//! 对应 `cuckoo_service::request_service::send_request`（`plan.md` M1.3 节）。
//! 接收 `SendRequestInput` JSON body，调用 Service 层执行 HTTP 请求并返回 `ExecutionResult`。

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_dto::{ExecutionResult, SendRequestInput};
use cuckoo_service::send_request;

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new().route("/api/requests/send", post(handle_send_request))
}

async fn handle_send_request(
    State(state): State<AuthState>,
    Json(input): Json<SendRequestInput>,
) -> Result<Json<ExecutionResult>, ServiceError> {
    // M3.3：注入 ProxyState，支持 via_proxy 经代理转发
    let result = send_request(&state.db, &state.proxy_state, input).await?;
    Ok(Json(result))
}
