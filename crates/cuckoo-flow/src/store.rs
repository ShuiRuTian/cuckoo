//! 环形缓冲存储：在内存中保存最近的 Flow 记录，供历史查询和详情拉取。
//!
//! M2 精简版：使用 `VecDeque` + 容量上限，先进先出。
//! 后续可升级为 LRU 或基于 SQLite 的持久化存储。

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::model::Flow;

/// 默认最大缓存的 Flow 数量。
const DEFAULT_MAX_FLOWS: usize = 5000;

/// 环形缓冲存储。
///
/// 所有 Flow 在 `on_request` 时插入，后续可通过 `get_flow()` 或
/// `query_flows()` 查询。超出容量上限时自动淘汰最早的记录。
#[derive(Clone)]
pub struct FlowStore {
    inner: Arc<RwLock<VecDeque<Flow>>>,
    max_flows: usize,
}

impl FlowStore {
    /// 创建默认容量（5000 条）的存储。
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_FLOWS)
    }

    /// 指定容量创建存储。
    pub fn with_capacity(max_flows: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(VecDeque::with_capacity(max_flows))),
            max_flows,
        }
    }

    /// 插入一条 Flow（新增或更新已有记录）。
    ///
    /// 从队尾向前查找（热点路径：`on_response` 更新的 Flow 刚创建不久、
    /// 位于队尾附近，通常 O(1) 命中；从队头找则是 O(n)）。
    pub fn upsert(&self, flow: Flow) {
        let mut guard = self.inner.write();
        // 从队尾向前找已有记录（按 id 匹配）
        if let Some(existing) = guard.iter_mut().rev().find(|f| f.id == flow.id) {
            *existing = flow;
        } else {
            // 新增：检查容量
            if guard.len() >= self.max_flows {
                guard.pop_front();
            }
            guard.push_back(flow);
        }
    }

    /// 按 ID 获取 Flow 的克隆。
    ///
    /// 从队尾向前找：最近创建的 Flow（最常被查询）在队尾附近。
    pub fn get(&self, id: &str) -> Option<Flow> {
        let guard = self.inner.read();
        guard.iter().rev().find(|f| f.id == id).cloned()
    }

    /// 查询最近的 Flow 列表（倒序，最新的在前）。
    ///
    /// `limit` 限制返回数量，`offset` 用于分页。
    pub fn query_recent(&self, limit: usize, offset: usize) -> Vec<Flow> {
        let guard = self.inner.read();
        guard
            .iter()
            .rev()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// 按域名过滤查询（倒序，最新的在前；`offset` 用于分页，
    /// 与 `query_recent` 语义一致）。
    pub fn query_by_host(&self, host_pattern: &str, limit: usize, offset: usize) -> Vec<Flow> {
        let guard = self.inner.read();
        guard
            .iter()
            .rev()
            .filter(|f| {
                // 从 request.uri 或 Host header 提取 host 做匹配
                let h = f
                    .request
                    .header("host")
                    .or_else(|| {
                        f.request
                            .uri
                            .split("://")
                            .nth(1)
                            .and_then(|s| s.split('/').next())
                    })
                    .unwrap_or("");
                h.contains(host_pattern)
            })
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// 当前存储的 Flow 数量。
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// 存储是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 清空所有记录。
    pub fn clear(&self) {
        self.inner.write().clear();
    }
}

impl Default for FlowStore {
    fn default() -> Self {
        Self::new()
    }
}
