//! 批量聚合器：内部 `mpsc` channel 收集 handler 产生的事件，
//! 16-50ms 窗口聚合后通过 `tokio::sync::broadcast` 对外暴露订阅接口
//! （`plan.md` M2.3 节）。
//!
//! 设计动机：代理内核在高流量场景下每秒可能产生数百个 Flow 事件，
//! 逐条 SSE 推送会导致前端 EventSource 过载。批量聚合后每批推送
//! 大幅减少 SSE 消息数量，同时保持低延迟（窗口 ≤ 50ms）。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::time::Instant;

use crate::model::TrafficEvent;
/// 默认批量聚合窗口（毫秒）。
const DEFAULT_BATCH_WINDOW_MS: u64 = 32;

/// 默认广播通道容量。
const DEFAULT_BROADCAST_CAPACITY: usize = 256;

/// 默认 mpsc 通道容量（handler → aggregator）。
const DEFAULT_MPSC_CAPACITY: usize = 1024;

/// 单批次最大事件数（超过则提前 flush）。
const MAX_BATCH_SIZE: usize = 64;

/// Flow 事件聚合器。
///
/// 内部启动一个 tokio task，从 `mpsc` receiver 收集事件，
/// 按时间窗口或数量阈值聚合为 `Vec<TrafficEvent>`，
/// 然后通过 `broadcast` channel 推送给所有订阅者。
pub struct FlowAggregator {
    /// 发送端：代理 handler 通过此 channel 产生事件。
    ///
    /// 包裹在 `Mutex<Option<...>>` 中：`shutdown()` 取出并 drop 自身持有的
    /// sender（handler 们的临时 clone 用完即弃），所有 sender 释放后
    /// channel 关闭，聚合 task flush 残余事件并退出。
    /// 若直接持有裸 sender，`drop(self.tx.clone())` 永远无法关闭 channel，
    /// `shutdown()` 会永久等待聚合 task 退出（死锁）。
    tx: std::sync::Mutex<Option<mpsc::Sender<TrafficEvent>>>,
    /// 广播接收端句柄：SSE 端点和 Service 层通过此 handle 订阅
    broadcast_tx: broadcast::Sender<Vec<TrafficEvent>>,
    /// 聚合 task 的 JoinHandle
    join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl FlowAggregator {
    /// 创建新的聚合器并启动后台聚合 task。
    pub fn new() -> Arc<Self> {
        Self::with_capacity(
            DEFAULT_MPSC_CAPACITY,
            DEFAULT_BROADCAST_CAPACITY,
            DEFAULT_BATCH_WINDOW_MS,
        )
    }

    /// 指定容量和窗口大小创建聚合器。
    pub fn with_capacity(
        mpsc_capacity: usize,
        broadcast_capacity: usize,
        window_ms: u64,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<TrafficEvent>(mpsc_capacity);
        let (broadcast_tx, _) = broadcast::channel::<Vec<TrafficEvent>>(broadcast_capacity);

        let agg_tx = broadcast_tx.clone();
        let join_handle = tokio::spawn(async move {
            aggregator_loop(rx, agg_tx, window_ms).await;
        });

        Arc::new(Self {
            tx: std::sync::Mutex::new(Some(tx)),
            broadcast_tx,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    /// 获取事件发送端（代理 handler 用）。
    ///
    /// 聚合器已 shutdown 后返回 `None`，事件将被调用方静默丢弃。
    pub fn sender(&self) -> Option<mpsc::Sender<TrafficEvent>> {
        self.tx.lock().expect("aggregator tx mutex poisoned").clone()
    }

    /// 订阅批量事件流。
    ///
    /// 每次调用返回一个新的 `broadcast::Receiver`，
    /// SSE 端点持有一个 receiver 来推送事件给客户端。
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<TrafficEvent>> {
        self.broadcast_tx.subscribe()
    }

    /// 关闭聚合器：flush 残余事件并等待聚合 task 退出。
    ///
    /// 取出并 drop 自身持有的 sender 后，mpsc channel 在所有临时
    /// clone 释放后关闭，聚合 task 收到 `None` flush 残余并退出。
    /// 注意：关闭后聚合器不再可用（`sender()` 返回 `None`），
    /// 因此只在应用退出时调用，代理启停不调用。
    pub async fn shutdown(&self) {
        {
            let mut guard = self.tx.lock().expect("aggregator tx mutex poisoned");
            *guard = None; // drop 自身持有的 sender
        }

        let mut guard = self.join_handle.lock().await;
        if let Some(handle) = guard.take() {
            let _ = handle.await;
        }
    }
}

/// 聚合循环：从 mpsc 收集事件，按窗口/数量聚合后 broadcast。
async fn aggregator_loop(
    mut rx: mpsc::Receiver<TrafficEvent>,
    broadcast_tx: broadcast::Sender<Vec<TrafficEvent>>,
    window_ms: u64,
) {
    let window = Duration::from_millis(window_ms);
    let mut batch: Vec<TrafficEvent> = Vec::with_capacity(MAX_BATCH_SIZE);

    loop {
        // 等待第一个事件到达（无超时，阻塞直到有事件或 channel 关闭）
        match rx.recv().await {
            Some(event) => batch.push(event),
            None => {
                // channel 关闭：flush 残余事件后退出
                if !batch.is_empty() {
                    let _ = broadcast_tx.send(std::mem::take(&mut batch));
                }
                tracing::debug!("flow aggregator: mpsc closed, exiting");
                return;
            }
        }

        // 第一个事件已入队，启动窗口定时器
        let deadline = Instant::now() + window;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());

            // 窗口已到（事件恰在 deadline 附近到达 / 定时器唤醒延迟）：
            // 必须立即 flush，否则 sleep 分支被禁用后 select 只剩 recv，
            // 不满一批的事件会永久滞留。
            if remaining.is_zero() {
                break;
            }

            tokio::select! {
                // 窗口超时：flush
                _ = tokio::time::sleep(remaining) => {
                    break;
                }

                // 收到新事件
                result = rx.recv() => {
                    match result {
                        Some(event) => {
                            batch.push(event);
                            // 达到批量上限：提前 flush
                            if batch.len() >= MAX_BATCH_SIZE {
                                break;
                            }
                        }
                        None => {
                            // channel 关闭：flush 残余
                            if !batch.is_empty() {
                                let _ = broadcast_tx.send(std::mem::take(&mut batch));
                            }
                            tracing::debug!("flow aggregator: mpsc closed during window, exiting");
                            return;
                        }
                    }
                }
            }
        }

        // flush 批次
        if !batch.is_empty() {
            let events = std::mem::take(&mut batch);
            if broadcast_tx.send(events).is_err() {
                // 没有活跃订阅者：静默丢弃（正常，无 SSE 客户端连接时）
                tracing::trace!("flow aggregator: no active subscribers, batch dropped");
            }
        }
    }
}
