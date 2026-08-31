//! `Authorization: Bearer <token>` 鉴权中间件（`spec.md` 2.2 节第 5 点）。
//!
//! 校验请求头 `Authorization: Bearer <token>`（或 SSE 场景下
//! `?token=` query 参数，浏览器 `EventSource` 原生不支持自定义请求头）
//! 是否等于启动时生成/复用的 token 文件内容。
//! `cuckoo-server` 只监听 `127.0.0.1`，token 是唯一的安全边界。

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use cuckoo_ca::CaAuthority;
use cuckoo_flow::{FlowAggregator, FlowStore};
use cuckoo_service::proxy_service::ProxyState;
use cuckoo_service::rule_service::RuleState;
use rand::distr::Alphanumeric;
use rand::RngExt;
use sea_orm::DatabaseConnection;
use serde::Deserialize;

/// 允许 `?token=` 查询参数鉴权的路由白名单。
///
/// 浏览器 `EventSource` 原生不支持自定义请求头，SSE 端点只能
/// 通过 query 参数携带 token；其余 REST 端点一律只认
/// `Authorization` 头，避免 token 进入访问日志/代理日志。
const QUERY_TOKEN_ALLOWED_PATHS: [&str; 1] = ["/api/flows/stream"];

/// 应用数据目录下的 token 文件路径（如 `~/Library/Application Support/Cuckoo/server.token`）。
pub fn token_file_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    base.join("Cuckoo").join("server.token")
}

/// 启动时生成/复用 token 文件内容（`spec.md` 2.2 节第 5 点）。
pub fn load_or_create_token() -> anyhow::Result<String> {
    let path = token_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let existing = fs::read_to_string(&path)?;
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let token: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    fs::write(&path, &token)?;

    // token 文件仅限当前用户读写（避免同机其他用户读取 token）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(token)
}

/// 鉴权中间件共享状态（同时持有鉴权 token、数据库连接、CA 证书管理器和 Flow 聚合器）。
#[derive(Clone)]
pub struct AuthState {
    pub token: Arc<String>,
    pub db: Arc<DatabaseConnection>,
    pub ca: Arc<CaAuthority>,
    /// Flow 事件聚合器（SSE 端点订阅此聚合器的 broadcast channel）
    pub flow_aggregator: Arc<FlowAggregator>,
    /// Flow 环形缓冲存储（供历史查询和详情拉取）
    pub flow_store: FlowStore,
    /// 代理状态（包含系统代理管理器）
    pub proxy_state: Arc<ProxyState>,
    /// 规则状态（M3 新增）
    pub rule_state: Arc<RuleState>,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// 常数时间字节比较（防止计时侧信道逐字节泄漏 token）。
///
/// 长度不等直接返回 false（token 长度本身不敏感，
/// 关键是相同长度时不因字节位置提前退出）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// `Authorization: Bearer <token>` 鉴权中间件。
///
/// REST 端点一律只认 `Authorization` 头；仅 SSE 端点
/// （`/api/flows/stream`）额外接受 `?token=` 查询参数——浏览器
/// `EventSource` 无法自定义请求头，见 `spec.md` 7.5 节。
pub async fn require_auth(
    State(state): State<AuthState>,
    Query(query): Query<TokenQuery>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header_token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    // query token 仅对 SSE 路由白名单生效
    let query_token_allowed = QUERY_TOKEN_ALLOWED_PATHS
        .contains(&request.uri().path());

    let provided = header_token.or({
        if query_token_allowed {
            query.token.as_deref()
        } else {
            None
        }
    });

    match provided {
        Some(t) if constant_time_eq(t.as_bytes(), state.token.as_bytes()) => {
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// 生成 CORS 层，放行所有来源。
///
/// `cuckoo-server` 只监听 `127.0.0.1` 且所有端点都要求 Bearer token，
/// Origin 不构成安全边界；桌面 UI 页面（`tauri://` 源）与 Vite dev server
/// （动态端口）发起的跨源请求需要浏览器侧放行，因此不做 Origin 限制。
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}
