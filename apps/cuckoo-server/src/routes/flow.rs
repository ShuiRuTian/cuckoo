//! Flow 路由：流量查询与详情拉取。
//!
//! 对应 `spec.md` 7.2 节的 Flow 相关 REST 端点：
//! - `GET /api/flows` — 查询历史 Flow（支持分页+过滤）
//! - `GET /api/flows/{id}` — 单条 Flow 详情
//! - `GET /api/flows/{id}/body?part=request|response` — 惰性拉取 body
//!
//! 数据来源是 `AuthState.flow_store`（环形缓冲存储）。

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_flow::{Flow, FlowBodyResponse, FlowListResponse};
use serde::Deserialize;

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/api/flows", get(handle_list_flows))
        .route("/api/flows/{id}", get(handle_get_flow))
        .route("/api/flows/{id}/body", get(handle_get_flow_body))
}

/// 查询参数：分页 + 域名过滤。
#[derive(Debug, Deserialize)]
struct ListFlowsQuery {
    /// 返回数量上限（默认 100）
    limit: Option<usize>,
    /// 偏移量（默认 0）
    offset: Option<usize>,
    /// 按域名模糊匹配
    host: Option<String>,
}

async fn handle_list_flows(
    State(state): State<AuthState>,
    Query(query): Query<ListFlowsQuery>,
) -> Result<Json<FlowListResponse>, ServiceError> {
    let limit = query.limit.unwrap_or(100).min(500);
    let offset = query.offset.unwrap_or(0);

    let flows = if let Some(host) = &query.host {
        state.flow_store.query_by_host(host, limit, offset)
    } else {
        state.flow_store.query_recent(limit, offset)
    };

    let total = state.flow_store.len();

    Ok(Json(FlowListResponse { flows, total }))
}

async fn handle_get_flow(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<Flow>, ServiceError> {
    state
        .flow_store
        .get(&id)
        .map(Json)
        .ok_or_else(|| ServiceError::NotFound(format!("flow not found: {id}")))
}

/// Body 拉取查询参数。
#[derive(Debug, Deserialize)]
struct BodyQuery {
    /// "request" 或 "response"
    part: String,
}

async fn handle_get_flow_body(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Query(query): Query<BodyQuery>,
) -> Result<Json<FlowBodyResponse>, ServiceError> {
    let flow = state
        .flow_store
        .get(&id)
        .ok_or_else(|| ServiceError::NotFound(format!("flow not found: {id}")))?;

    let msg = match query.part.as_str() {
        "request" => &flow.request,
        "response" => flow
            .response
            .as_ref()
            .ok_or_else(|| ServiceError::NotFound("response not available".to_string()))?,
        other => {
            return Err(ServiceError::BadRequest(format!(
                "invalid part '{other}', expected 'request' or 'response'"
            )))
        }
    };

    // UTF-8 文本直接返回；二进制内容用标准 base64 编码并显式标记，
    // 前端按 `binary` 字段决定是否解码
    let (body, binary) = match String::from_utf8(msg.body.clone()) {
        Ok(text) => (text, false),
        Err(_) => {
            use base64::Engine as _;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&msg.body);
            (encoded, true)
        }
    };

    let content_type = msg
        .header("content-type")
        .map(String::from);

    Ok(Json(FlowBodyResponse {
        body,
        body_size: msg.body_size,
        body_truncated: msg.body_truncated,
        content_type,
        binary,
    }))
}
