//! `cuckoo-http`：HTTP 客户端引擎（`reqwest` 封装 + 精确计时）。
//!
//! `RequestExecutor::execute()` 支持 method/url/headers/query params/body。
//! M1 阶段先做 Raw JSON body 和粗粒度计时（total time），
//! DNS/TLS 精细阶段放到 M5 打磨阶段。
//!
//! `ExecuteRequestInput` 和 `ExecutionResult` 类型定义在 `cuckoo-dto` 中，
//! 本 crate 只负责执行逻辑。

use std::collections::HashMap;
use std::time::Instant;

use cuckoo_core::ServiceError;
use cuckoo_dto::{ExecuteRequestInput, ExecutionResult};
use cuckoo_store::entities::http_request_def::{AuthConfig, RequestBody};

/// HTTP 请求执行器：封装 `reqwest::Client`。
pub struct RequestExecutor {
    client: reqwest::Client,
}

impl Default for RequestExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestExecutor {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // M1 先放宽 TLS 校验，后续由 WorkspaceSettings 控制
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");

        Self { client }
    }

    /// M3.3：构造经本地代理转发的执行器。
    ///
    /// 用于“Collection 请求经代理发送”联动：请求流量经过 Cuckoo 自身
    /// 的 MITM 代理，可被拦截规则（Rewrite/断点等）处理后在 UI 流量列表中呈现。
    /// 代理的自签证书由 `danger_accept_invalid_certs` 兼容。
    pub fn with_proxy(proxy_addr: &str) -> Self {
        let proxy_url = if proxy_addr.starts_with("http://") {
            proxy_addr.to_string()
        } else {
            format!("http://{proxy_addr}")
        };
        let proxy = reqwest::Proxy::all(&proxy_url)
            .expect("invalid proxy url");
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true) // 兼容 MITM 代理的自签证书
            .timeout(std::time::Duration::from_secs(30))
            .proxy(proxy)
            .build()
            .expect("failed to build reqwest client with proxy");

        Self { client }
    }

    /// 执行一个 HTTP 请求。
    pub async fn execute(&self, input: &ExecuteRequestInput) -> Result<ExecutionResult, ServiceError> {
        let start = Instant::now();

        // 解析 method
        let method = reqwest::Method::from_bytes(input.method.as_bytes())
            .map_err(|e| ServiceError::BadRequest(format!("invalid method: {e}")))?;

        // 构建 request builder
        let mut builder = self.client.request(method, &input.url);

        // 添加 query params
        if !input.query_params.is_empty() {
            let mut params = Vec::new();
            for kv in &input.query_params {
                if kv.enabled {
                    params.push((&kv.key, &kv.value));
                }
            }
            if !params.is_empty() {
                builder = builder.query(&params);
            }
        }

        // 添加 headers
        for header in &input.headers {
            if header.enabled {
                builder = builder.header(&header.name, &header.value);
            }
        }

        // 处理 Auth（DTO → store 类型转换）
        let auth_json = serde_json::to_value(&input.auth).unwrap_or_default();
        let auth: AuthConfig = serde_json::from_value(auth_json).unwrap_or_default();
        builder = apply_auth(builder, &auth);

        // 处理 body（DTO → store 类型转换）
        let body_json = serde_json::to_value(&input.body).unwrap_or_default();
        let body: RequestBody = serde_json::from_value(body_json).unwrap_or_default();
        builder = apply_body(builder, &body);

        // 发送请求
        let response = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ExecutionResult {
                    status: 0,
                    status_text: "Request Failed".to_string(),
                    headers: HashMap::new(),
                    body: String::new(),
                    body_size: 0,
                    content_type: None,
                    total_time_ms: start.elapsed().as_millis() as u64,
                    success: false,
                    error: Some(format!("{e}")),
                });
            }
        };

        let status = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("").to_string();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // 收集 headers
        let mut resp_headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                resp_headers.insert(key.as_str().to_string(), v.to_string());
            }
        }

        // 读取 body
        let body_bytes = response.bytes().await.map_err(|e| {
            ServiceError::Internal(format!("failed to read response body: {e}"))
        })?;
        let body_size = body_bytes.len();
        let body_text = String::from_utf8_lossy(&body_bytes).to_string();

        let total_time_ms = start.elapsed().as_millis() as u64;
        let success = (200..300).contains(&status);

        Ok(ExecutionResult {
            status,
            status_text,
            headers: resp_headers,
            body: body_text,
            body_size,
            content_type,
            total_time_ms,
            success,
            error: None,
        })
    }
}

fn apply_auth(builder: reqwest::RequestBuilder, auth: &AuthConfig) -> reqwest::RequestBuilder {
    match auth {
        AuthConfig::None => builder,
        AuthConfig::Basic { username, password } => {
            builder.basic_auth(username, Some(password))
        }
        AuthConfig::Bearer { token } => builder.bearer_auth(token),
        AuthConfig::ApiKey { key_name, key_value, add_to } => {
            if add_to == "query" {
                builder.query(&[(key_name.as_str(), key_value.as_str())])
            } else {
                builder.header(key_name, key_value)
            }
        }
    }
}

fn apply_body(builder: reqwest::RequestBuilder, body: &RequestBody) -> reqwest::RequestBuilder {
    match body {
        RequestBody::None => builder,
        RequestBody::Raw { content_type, text } => {
            if text.is_empty() {
                return builder;
            }
            // 尝试解析为 JSON 如果 content_type 是 json
            if content_type.contains("json") {
                match serde_json::from_str::<serde_json::Value>(text) {
                    Ok(json_val) => builder.json(&json_val),
                    Err(_) => builder.body(text.clone()).header("Content-Type", content_type),
                }
            } else {
                builder.body(text.clone()).header("Content-Type", content_type)
            }
        }
    }
}
