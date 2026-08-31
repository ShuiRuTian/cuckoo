//! M0 最小闭环验证用的 `ping` 方法（`plan.md` M0 最后一条任务）。
//!
//! 目的：验证"写一次 Service 方法，自动出现 REST 端点"以及"页面经 `tauri://`
//! 加载、业务请求经 `cuckoo-server`"这两条链路端到端可用。真正的业务 Service
//! 方法（`request_service`/`collection_service`/... ）从 M1 起陆续在这里补齐。

use cuckoo_dto::PongResponse;
use cuckoo_macros::rpc_method;

/// Service 层唯一的业务逻辑方法：M0 阶段只是简单返回一句问候 + 服务器时间。
#[rpc_method("GET", "/api/ping")]
pub async fn ping() -> PongResponse {
    let server_time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();

    PongResponse {
        message: "pong from cuckoo-service".to_string(),
        server_time_ms,
    }
}
