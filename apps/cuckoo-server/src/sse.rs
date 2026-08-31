//! SSE 事件推送端点（`/api/flows/stream`，`spec.md` 6.3/7.2 节）。
//!
//! M2.3 阶段：订阅 `cuckoo-flow` 的 `FlowAggregator` broadcast channel，
//! 把批量事件序列化为 `flow.batch` SSE 消息推送给所有连接的客户端
//! （桌面 UI/CLI/MCP 共用同一端点）。

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{self, Stream};

use crate::auth::AuthState;

/// `GET /api/flows/stream` 的 SSE 端点。
///
/// 订阅 `FlowAggregator` 的 broadcast channel，把每个批次序列化为
/// `flow.batch` SSE 事件推送给客户端。
///
/// 客户端通过 `EventSource` 订阅，解析 `flow.batch` 事件得到
/// `TrafficEvent[]`（`spec.md` 6.3 节）。
pub async fn flow_stream(
    State(state): State<AuthState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.flow_aggregator.subscribe();

    // 把 broadcast::Receiver 转换为 SSE 事件流。
    //
    // unfold 的状态是 (receiver, ended)：
    // - 正常路径：yield flow.batch 事件，继续下一轮
    // - Lagged：跳过丢失的批次，继续循环
    // - Closed：yield 一次 flow.end 事件并把 ended 置 true，
    //   下一轮返回 None 终止流。若直接把同一个 rx 传回，下一轮
    //   recv() 会立即再次返回 Closed，形成无限紧密循环
    //   （CPU 打满 + flow.end 消息洪水）。
    let stream = stream::unfold((rx, false), |(mut rx, ended)| async move {
        if ended {
            // 上一轮已发送 flow.end，正式终止流
            return None;
        }

        loop {
            match rx.recv().await {
                Ok(batch) => {
                    let data = serde_json::to_string(&batch).unwrap_or_else(|e| {
                        tracing::warn!(?e, "failed to serialize flow batch");
                        "[]".to_string()
                    });

                    let event = Event::default().event("flow.batch").data(data);

                    return Some((Ok(event), (rx, false)));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE subscriber lagged, skipping batches");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // 聚合器已关闭：发送 flow.end 后终止流
                    let event = Event::default()
                        .event("flow.end")
                        .data(r#"{"reason":"aggregator closed"}"#);
                    return Some((Ok(event), (rx, true)));
                }
            }
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// M0 阶段的占位实现（保留兼容，后续移除）。
pub async fn flow_stream_placeholder() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(0u64, |count| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let event = Event::default()
            .event("heartbeat")
            .data(format!("{{\"seq\":{count}}}"));
        Some((Ok(event), count + 1))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
