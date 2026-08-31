//! `request_service`：HTTP 请求发送方法（`spec.md` 7.2 节，`plan.md` M1.3 节）。
//!
//! `send_request` 支持两种模式：
//! - 按 `request_id` 从数据库加载已保存的 `HttpRequestDef`，经过模板插值后发送；
//! - 直接传入 ad-hoc 请求参数（不经数据库），立即发送。
//!
//! 环境变量替换：如果传入了 `environment_id`，从数据库加载该 Environment 的变量列表，
//! 用 `cuckoo-templates::VariableContext` 对 URL/Headers/QueryParams/Body 做插值。

use cuckoo_core::{ServiceError, ServiceResult};
use cuckoo_dto::{ExecutionResult, ExecuteRequestInput, SendRequestInput};
use cuckoo_http::RequestExecutor;
use cuckoo_macros::rpc_method;
use crate::proxy_service::ProxyState;
use cuckoo_store::entities::http_request_def::RequestBody;
use cuckoo_store::entities::workspace::HeaderEntry;
use cuckoo_store::entities::http_request_def::KeyValueEntry;
use cuckoo_store::repo::{environment_repo, request_repo};
use cuckoo_templates::VariableContext;
use sea_orm::DatabaseConnection;

/// 发送一个 HTTP 请求（已保存的或 ad-hoc），返回执行结果。
///
/// 标注为 `POST /api/requests/send`（`spec.md` 7.2 节）。
///
/// M3.3：`input.via_proxy == Some(true)` 且代理运行中时，请求经本地 MITM
/// 代理转发（可被拦截规则处理，并在流量列表中呈现）。
#[rpc_method("POST", "/api/requests/send")]
pub async fn send_request(
    db: &DatabaseConnection,
    proxy: &ProxyState,
    input: SendRequestInput,
) -> ServiceResult<ExecutionResult> {
    // 1. 确定 HTTP 请求参数来源
    let (method, url, headers, query_params, body, auth) = if let Some(id) = &input.request_id {
        // 从数据库加载已保存的请求定义
        let req = request_repo::find_by_id(db, id)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("request {id}")))?;

        (
            req.method,
            req.url,
            req.headers,
            req.query_params,
            req.body,
            req.auth,
        )
    } else if let Some(ad_hoc) = &input.ad_hoc {
        (
            ad_hoc.method.clone(),
            ad_hoc.url.clone(),
            ad_hoc.headers_json(),
            ad_hoc.query_params_json(),
            ad_hoc.body_json(),
            ad_hoc.auth_json(),
        )
    } else {
        return Err(ServiceError::BadRequest(
            "either request_id or ad_hoc must be provided".to_string(),
        ));
    };

    // 2. 如果提供了 environment_id，加载环境变量做插值
    let mut final_url = url;
    let mut final_headers = headers;
    let mut final_query_params = query_params;
    let mut final_body = body;

    if let Some(env_id) = &input.environment_id {
        let env = environment_repo::find_by_id(db, env_id)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?
            .ok_or_else(|| ServiceError::NotFound(format!("environment {env_id}")))?;

        let ctx = VariableContext::from_env_json(&env.variables);

        // 渲染 URL
        final_url = ctx.render_url(&final_url);

        // 渲染 Headers
        let mut header_list: Vec<HeaderEntry> =
            serde_json::from_value(final_headers.clone()).unwrap_or_default();
        ctx.render_headers(&mut header_list);
        final_headers = serde_json::to_value(&header_list).unwrap_or(final_headers);

        // 渲染 QueryParams
        let mut qp_list: Vec<KeyValueEntry> =
            serde_json::from_value(final_query_params.clone()).unwrap_or_default();
        ctx.render_query_params(&mut qp_list);
        final_query_params = serde_json::to_value(&qp_list).unwrap_or(final_query_params);

        // 渲染 Body（仅 Raw 类型）
        let body_obj: RequestBody =
            serde_json::from_value(final_body.clone()).unwrap_or_default();
        if let RequestBody::Raw { content_type, text } = body_obj {
            let rendered_text = ctx.render_body_text(&text);
            final_body = serde_json::to_value(&RequestBody::Raw {
                content_type,
                text: rendered_text,
            })
            .unwrap_or(final_body);
        }
    }

    // 3. 构造 ExecuteRequestInput 并执行
    // ExecuteRequestInput 中的字段是强类型（Vec<HeaderEntry> 等），
    // 从 JSON Value 反序列化得到。
    let exec_input = ExecuteRequestInput {
        method,
        url: final_url,
        headers: serde_json::from_value(final_headers).unwrap_or_default(),
        query_params: serde_json::from_value(final_query_params).unwrap_or_default(),
        body: serde_json::from_value(final_body).unwrap_or_default(),
        auth: serde_json::from_value(auth).unwrap_or_default(),
    };

    // 3. 选择执行器：可选经本地代理转发（M3.3 联动）
    let executor = if input.via_proxy.unwrap_or(false) {
        let guard = proxy.proxy_server.lock().await;
        match guard.as_ref() {
            Some(server) => {
                let addr = server.listen_addr.to_string();
                drop(guard);
                RequestExecutor::with_proxy(&addr)
            }
            None => {
                return Err(ServiceError::BadRequest(
                    "via_proxy is set but proxy is not running; start the proxy first".to_string(),
                ));
            }
        }
    } else {
        RequestExecutor::new()
    };
    executor
        .execute(&exec_input)
        .await
        .map_err(|e| ServiceError::Internal(e.to_string()))
}
