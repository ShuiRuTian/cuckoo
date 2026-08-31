//! Rule 路由：拦截规则 CRUD + 断点恢复。
//!
//! 对应 `cuckoo_service::rule_service` 的 `create_rule` / `list_rules` /
//! `get_rule` / `update_rule` / `delete_rule` / `clear_rules` /
//! `resume_intercepted_flow` / `list_pending_intercepts` 方法
//! （`plan.md` M3.1 节）。

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use cuckoo_core::ServiceError;
use cuckoo_proxy::{InterceptDecision, RuleEntry};
use cuckoo_service::rule_service::{
    CreateRuleInput, PendingIntercept, PendingInterceptDetail, UpdateRuleInput, clear_rules,
    create_rule, delete_rule, get_intercept, get_rule, list_pending_intercepts, list_rules,
    resume_intercepted_flow, update_rule,
};

use crate::auth::AuthState;

pub fn router() -> Router<AuthState> {
    Router::new()
        .route("/api/rules", post(handle_create_rule))
        .route("/api/rules", get(handle_list_rules))
        .route("/api/rules", delete(handle_clear_rules))
        .route("/api/rules/{id}", get(handle_get_rule))
        .route("/api/rules/{id}", put(handle_update_rule))
        .route("/api/rules/{id}", delete(handle_delete_rule))
        .route("/api/intercepts", get(handle_list_pending_intercepts))
        .route("/api/intercepts/{id}", get(handle_get_intercept))
        .route("/api/intercepts/{id}/resume", post(handle_resume_intercept))
}

async fn handle_create_rule(
    State(state): State<AuthState>,
    Json(input): Json<CreateRuleInput>,
) -> Result<Json<RuleEntry>, ServiceError> {
    let entry = create_rule(input, &state.rule_state).await?;
    Ok(Json(entry))
}

async fn handle_list_rules(
    State(state): State<AuthState>,
) -> Result<Json<Vec<RuleEntry>>, ServiceError> {
    let rules = list_rules(&state.rule_state).await?;
    Ok(Json(rules))
}

async fn handle_get_rule(
    State(state): State<AuthState>,
    Path(rule_id): Path<String>,
) -> Result<Json<RuleEntry>, ServiceError> {
    let entry = get_rule(rule_id, &state.rule_state).await?;
    Ok(Json(entry))
}

async fn handle_update_rule(
    State(state): State<AuthState>,
    Path(rule_id): Path<String>,
    Json(input): Json<UpdateRuleInput>,
) -> Result<Json<RuleEntry>, ServiceError> {
    let entry = update_rule(rule_id, input, &state.rule_state).await?;
    Ok(Json(entry))
}

async fn handle_delete_rule(
    State(state): State<AuthState>,
    Path(rule_id): Path<String>,
) -> Result<Json<()>, ServiceError> {
    delete_rule(rule_id, &state.rule_state).await?;
    Ok(Json(()))
}

async fn handle_clear_rules(
    State(state): State<AuthState>,
) -> Result<Json<()>, ServiceError> {
    clear_rules(&state.rule_state).await?;
    Ok(Json(()))
}

async fn handle_list_pending_intercepts(
    State(state): State<AuthState>,
) -> Result<Json<Vec<PendingIntercept>>, ServiceError> {
    let pending = list_pending_intercepts(&state.rule_state).await?;
    Ok(Json(pending))
}

async fn handle_get_intercept(
    State(state): State<AuthState>,
    Path(id): Path<String>,
) -> Result<Json<PendingInterceptDetail>, ServiceError> {
    let detail = get_intercept(id, &state.rule_state).await?;
    Ok(Json(detail))
}

async fn handle_resume_intercept(
    State(state): State<AuthState>,
    Path(id): Path<String>,
    Json(decision): Json<InterceptDecision>,
) -> Result<Json<()>, ServiceError> {
    resume_intercepted_flow(id, decision, &state.rule_state).await?;
    Ok(Json(()))
}
