//! 断点拦截与恢复（`spec.md` 4.5 节，`plan.md` M3.1 节）。
//!
//! 参考 mitmproxy 的 `flow.intercept()` / `wait_for_resume()`：
//! 用 `tokio::sync::oneshot` 实现"暂停当前请求处理协程，等待前端发来放行/修改/丢弃指令"。
//!
//! 核心类型：
//! - [`InterceptDecision`]：前端的决策（放行/丢弃/中断连接）
//! - [`InterceptRegistry`]：管理所有挂起中的 Flow，提供 `pause_and_wait` / `resolve` 方法

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

use crate::handler::HttpMessage;

/// 前端对断点的决策。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum InterceptDecision {
    /// 放行：前端可能修改过 header/body
    Continue {
        /// 修改后的消息（如果是 response 阶段断点，这里是 response；request 阶段则是 request）
        edited: Option<HttpMessage>,
    },
    /// 丢弃：返回一个空响应给客户端
    Abort,
    /// 直接中断 TCP 连接
    DropConnection,
}

/// 断点挂起时等待的 oneshot Sender。
type PendingSender = oneshot::Sender<InterceptDecision>;

/// 断点注册表：管理所有挂起中的 Flow。
///
/// 每个 Flow 被 `pause_and_wait` 挂起后，会插入一个 `oneshot::Sender`，
/// 前端调用 `resume_intercepted_flow(id, decision)` 时取出并发送决策。
pub struct InterceptRegistry {
    /// flow_id → oneshot Sender
    pending: DashMap<String, PendingSender>,
    /// flow_id → 挂起阶段 ("request" / "response")
    stages: DashMap<String, String>,
    /// flow_id → 原始消息（供前端预览）
    originals: DashMap<String, HttpMessage>,
}

/// 挂起条目的 RAII 清理 guard：drop 时移除三个 map 中的条目。
///
/// 保证 `pause_and_wait` 在任何退出路径（正常/错误/协程被 abort）
/// 下都不泄漏条目。
struct PendingGuard<'a> {
    registry: &'a InterceptRegistry,
    flow_id: String,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.registry.pending.remove(&self.flow_id);
        self.registry.stages.remove(&self.flow_id);
        self.registry.originals.remove(&self.flow_id);
    }
}

impl InterceptRegistry {
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
            stages: DashMap::new(),
            originals: DashMap::new(),
        }
    }

    /// 挂起当前协程并等待前端决策。
    ///
    /// 调用者传入 flow_id 和原始消息，此方法会：
    /// 1. 创建 oneshot channel
    /// 2. 将 Sender 存入 pending map
    /// 3. await Receiver，阻塞当前协程直到前端发来决策
    ///
    /// 返回：
    /// - `Ok(InterceptDecision)` — 前端发来了决策
    /// - `Err(...)` — oneshot 被 drop（如代理停止时 cancel_all、连接中断）
    ///
    /// 挂起期间通过 RAII guard 保证三个 map 的条目在**任何退出路径**下
    /// 都被清理（包括：连接中断导致挂起协程被 abort、未来被 drop），
    /// 避免残留条目一直出现在 `list_pending` 里给前端。
    pub async fn pause_and_wait(
        &self,
        flow_id: &str,
        stage: &str,
        original: HttpMessage,
    ) -> Result<InterceptDecision, InterceptError> {
        let (tx, rx) = oneshot::channel::<InterceptDecision>();

        // 存入 pending map
        self.pending.insert(flow_id.to_string(), tx);
        self.stages.insert(flow_id.to_string(), stage.to_string());
        self.originals.insert(flow_id.to_string(), original);

        tracing::info!(%flow_id, %stage, "flow paused at breakpoint, waiting for decision");

        // RAII guard：无论正常返回、错误返回还是协程被 abort（future 被 drop），
        // 都清理三个 map 的条目。cancel_all 先行移除时 remove 是无害的空操作。
        let _guard = PendingGuard {
            registry: self,
            flow_id: flow_id.to_string(),
        };

        // 阻塞等待
        match rx.await {
            Ok(decision) => {
                tracing::info!(%flow_id, "flow resumed from breakpoint");
                Ok(decision)
            }
            Err(_closed) => {
                // Sender 被 drop（如 cancel_all / 连接中断）
                tracing::warn!(%flow_id, "breakpoint cancelled (sender dropped)");
                Err(InterceptError::Cancelled)
            }
        }
        // _guard 在此 drop，自动清理三个 map
    }

    /// 前端调用此方法恢复一个被挂起的 Flow。
    ///
    /// 返回 `Ok(())` 表示成功发送决策，`Err` 表示 flow_id 不存在或已被处理。
    ///
    /// 注意：发送成功后 `stages`/`originals` 的清理由 `pause_and_wait`
    /// 内部的 RAII guard 负责（rx 唤醒时 guard drop）；发送失败
    /// （receiver 已被 drop，挂起协程已不存在）时在此同步清理残留条目。
    pub fn resolve(
        &self,
        flow_id: &str,
        decision: InterceptDecision,
    ) -> Result<(), InterceptError> {
        // 验证 stage 存在
        let stage = self
            .stages
            .get(flow_id)
            .map(|s| s.value().clone())
            .ok_or(InterceptError::NotFound(flow_id.to_string()))?;

        tracing::info!(%flow_id, %stage, "resolving breakpoint");

        // 取出 Sender
        let sender = self
            .pending
            .remove(flow_id)
            .ok_or(InterceptError::NotFound(flow_id.to_string()))?
            .1;

        // 发送决策
        match sender.send(decision) {
            Ok(()) => Ok(()),
            Err(_) => {
                // receiver 已被 drop（挂起协程被 abort）：同步清理残留条目，
                // 否则 flow 会永久留在 list_pending 里误导前端
                self.stages.remove(flow_id);
                self.originals.remove(flow_id);
                Err(InterceptError::AlreadyResolved(flow_id.to_string()))
            }
        }
    }

    /// 获取被挂起的 Flow 的原始消息（供前端预览编辑）。
    pub fn get_original(&self, flow_id: &str) -> Option<HttpMessage> {
        self.originals.get(flow_id).map(|r| r.clone())
    }

    /// 获取被挂起的 Flow 的阶段信息。
    pub fn get_stage(&self, flow_id: &str) -> Option<String> {
        self.stages.get(flow_id).map(|r| r.value().clone())
    }

    /// 获取所有挂起中的 Flow ID 列表。
    pub fn list_pending(&self) -> Vec<(String, String)> {
        self.stages
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// 取消所有挂起的 Flow（在代理停止时调用）。
    pub fn cancel_all(&self) {
        // 对每个 pending 的 sender 直接 drop，pause_and_wait 的 await 会返回 Err
        let ids: Vec<String> = self.pending.iter().map(|r| r.key().clone()).collect();
        for id in ids {
            self.pending.remove(&id);
            self.stages.remove(&id);
            self.originals.remove(&id);
        }
    }
}

impl Default for InterceptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 断点操作错误。
#[derive(Debug, Clone, thiserror::Error)]
pub enum InterceptError {
    #[error("intercept not found: {0}")]
    NotFound(String),

    #[error("intercept already resolved or cancelled: {0}")]
    AlreadyResolved(String),

    #[error("intercept cancelled (sender dropped)")]
    Cancelled,
}

/// 共享 InterceptRegistry 的类型别名。
pub type SharedInterceptRegistry = Arc<InterceptRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pause_and_resolve() {
        let registry = Arc::new(InterceptRegistry::new());

        let msg = HttpMessage {
            method: "GET".to_string(),
            uri: "/test".to_string(),
            version: "HTTP/1.1".to_string(),
            headers: vec![],
            body: Vec::new(),
        };

        // 在另一个 task 中 pause
        let registry_clone = registry.clone();
        let handle = tokio::spawn(async move {
            let result = registry_clone
                .pause_and_wait("flow-1", "request", msg)
                .await;
            result
        });

        // 确认 pending 存在
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        assert_eq!(registry.get_stage("flow-1").as_deref(), Some("request"));

        // resolve
        registry
            .resolve("flow-1", InterceptDecision::Abort)
            .unwrap();

        // 等待 task 完成
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        match result.unwrap() {
            InterceptDecision::Abort => {}
            _ => panic!("expected Abort"),
        }
    }

    #[test]
    fn test_resolve_not_found() {
        let registry = InterceptRegistry::new();
        let result = registry.resolve("nonexistent", InterceptDecision::Abort);
        assert!(matches!(result, Err(InterceptError::NotFound(_))));
    }
}
