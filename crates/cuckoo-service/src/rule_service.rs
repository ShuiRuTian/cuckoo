//! `rule_service`：拦截规则 CRUD + 断点恢复（`spec.md` 3.4/4.5 节，`plan.md` M3.1 节）。
//!
//! 提供：
//! - 规则的增删改查（`#[rpc_method]` 暴露为 `/api/rules` CRUD）
//! - `resume_intercepted_flow`（暴露为 `POST /api/intercepts/{id}/resume`，见 `spec.md` 4.5 节）
//! - 查询挂起中的断点列表

use std::sync::Arc;

use cuckoo_core::{ServiceError, ServiceResult};
use cuckoo_macros::rpc_method;
use cuckoo_proxy::{
    HttpMessage, InterceptDecision, InterceptRegistry, RuleEngine, RuleEntry,
    SharedInterceptRegistry, SharedRuleEngine,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ────────────────────────────────────────────────────────────────────
// DTO 类型
// ────────────────────────────────────────────────────────────────────

/// 规则服务状态（运行时持有）。
pub struct RuleState {
    /// 规则引擎（与 ProxyState 共享同一实例）
    pub rule_engine: SharedRuleEngine,
    /// 断点注册表（与 ProxyState 共享同一实例）
    pub intercept_registry: SharedInterceptRegistry,
}

impl RuleState {
    pub fn new(
        rule_engine: SharedRuleEngine,
        intercept_registry: SharedInterceptRegistry,
    ) -> Self {
        Self {
            rule_engine,
            intercept_registry,
        }
    }

    /// 从 ProxyState 创建（共享同一实例）。
    pub fn from_proxy_state(
        rule_engine: Arc<RuleEngine>,
        intercept_registry: Arc<InterceptRegistry>,
    ) -> Self {
        Self::new(rule_engine, intercept_registry)
    }
}

/// 创建规则的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct CreateRuleInput {
    /// 规则名称
    pub name: String,
    /// 规则内容
    pub rule: cuckoo_proxy::InterceptRule,
    /// 排序键（越小越先匹配，默认 1.0）
    pub sort_key: Option<f64>,
}

/// 更新规则的输入参数。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct UpdateRuleInput {
    pub name: Option<String>,
    pub rule: Option<cuckoo_proxy::InterceptRule>,
    pub sort_key: Option<f64>,
}

/// 挂起中的断点信息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct PendingIntercept {
    pub flow_id: String,
    pub stage: String,
}

/// 挂起断点的详情（含原始消息，供前端编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct PendingInterceptDetail {
    pub flow_id: String,
    /// 挂起阶段："request" / "response"
    pub stage: String,
    /// 被挂起的原始消息（request 阶段为请求，response 阶段为响应）
    pub original: HttpMessage,
}

// ────────────────────────────────────────────────────────────────────
// Service 方法
// ────────────────────────────────────────────────────────────────────

/// 创建拦截规则。
///
/// 标注为 `POST /api/rules`。
#[rpc_method("POST", "/api/rules")]
pub async fn create_rule(input: CreateRuleInput, state: &RuleState) -> ServiceResult<RuleEntry> {
    let id = ulid::Ulid::new().to_string();
    let entry = RuleEntry {
        id,
        name: input.name,
        rule: input.rule,
        sort_key: input.sort_key.unwrap_or(1.0),
    };
    state.rule_engine.upsert(entry.clone());
    tracing::info!(rule_id = %entry.id, name = %entry.name, "rule created");
    Ok(entry)
}

/// 获取所有拦截规则。
///
/// 标注为 `GET /api/rules`。
#[rpc_method("GET", "/api/rules")]
pub async fn list_rules(state: &RuleState) -> ServiceResult<Vec<RuleEntry>> {
    Ok(state.rule_engine.list())
}

/// 获取单条规则。
///
/// 标注为 `GET /api/rules/{id}`。
#[rpc_method("GET", "/api/rules/{id}")]
pub async fn get_rule(id: String, state: &RuleState) -> ServiceResult<RuleEntry> {
    state
        .rule_engine
        .get(&id)
        .ok_or_else(|| ServiceError::NotFound(format!("rule not found: {id}")))
}

/// 更新规则。
///
/// 标注为 `PUT /api/rules/{id}`。
#[rpc_method("PUT", "/api/rules/{id}")]
pub async fn update_rule(
    id: String,
    input: UpdateRuleInput,
    state: &RuleState,
) -> ServiceResult<RuleEntry> {
    let mut entry = state
        .rule_engine
        .get(&id)
        .ok_or_else(|| ServiceError::NotFound(format!("rule not found: {id}")))?;

    if let Some(name) = input.name {
        entry.name = name;
    }
    if let Some(rule) = input.rule {
        entry.rule = rule;
    }
    if let Some(sort_key) = input.sort_key {
        entry.sort_key = sort_key;
    }

    state.rule_engine.upsert(entry.clone());
    tracing::info!(rule_id = %entry.id, "rule updated");
    Ok(entry)
}

/// 删除规则。
///
/// 标注为 `DELETE /api/rules/{id}`。
#[rpc_method("DELETE", "/api/rules/{id}")]
pub async fn delete_rule(id: String, state: &RuleState) -> ServiceResult<()> {
    state
        .rule_engine
        .remove(&id)
        .map(|_| {
            tracing::info!(%id, "rule deleted");
        })
        .ok_or_else(|| ServiceError::NotFound(format!("rule not found: {id}")))
}

/// 清空所有规则。
///
/// 标注为 `DELETE /api/rules`。
#[rpc_method("DELETE", "/api/rules")]
pub async fn clear_rules(state: &RuleState) -> ServiceResult<()> {
    state.rule_engine.clear();
    tracing::info!("all rules cleared");
    Ok(())
}

/// 恢复一个被挂起的 Flow（断点放行/修改/丢弃）。
///
/// 标注为 `POST /api/intercepts/{id}/resume`（`spec.md` 4.5 节）。
#[rpc_method("POST", "/api/intercepts/{id}/resume")]
pub async fn resume_intercepted_flow(
    id: String,
    decision: InterceptDecision,
    state: &RuleState,
) -> ServiceResult<()> {
    state
        .intercept_registry
        .resolve(&id, decision)
        .map_err(|e| ServiceError::Internal(format!("failed to resume intercept {id}: {e}")))?;

    tracing::info!(flow_id = %id, "intercept resumed");
    Ok(())
}

/// 获取所有挂起中的断点列表。
///
/// 标注为 `GET /api/intercepts`。
#[rpc_method("GET", "/api/intercepts")]
pub async fn list_pending_intercepts(state: &RuleState) -> ServiceResult<Vec<PendingIntercept>> {
    Ok(state
        .intercept_registry
        .list_pending()
        .into_iter()
        .map(|(flow_id, stage)| PendingIntercept { flow_id, stage })
        .collect())
}

/// 获取单个挂起断点的详情（含原始消息，供前端编辑界面展示）。
///
/// 标注为 `GET /api/intercepts/{id}`。
#[rpc_method("GET", "/api/intercepts/{id}")]
pub async fn get_intercept(id: String, state: &RuleState) -> ServiceResult<PendingInterceptDetail> {
    let stage = state
        .intercept_registry
        .get_stage(&id)
        .ok_or_else(|| ServiceError::NotFound(format!("intercept not found: {id}")))?;
    let original = state
        .intercept_registry
        .get_original(&id)
        .ok_or_else(|| ServiceError::NotFound(format!("intercept not found: {id}")))?;

    Ok(PendingInterceptDetail {
        flow_id: id,
        stage,
        original,
    })
}
