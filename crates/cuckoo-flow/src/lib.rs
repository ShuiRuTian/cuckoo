//! `cuckoo-flow`：Flow/Transaction 数据模型 + 批量聚合器 + 环形缓冲存储。
//!
//! 基于 `spec.md` 3.3 节和 `plan.md` M2.3 节的设计：
//! - [`model`]：`Flow`/`HttpMessage`/`FlowTiming`/`TlsInfo`/`TrafficEvent` 等 `#[ts(export)]` 类型定义
//! - [`aggregator`]：批量聚合器（mpsc → broadcast，16-50ms 窗口）
//! - [`store`]：环形缓冲存储（`VecDeque` + 容量上限）

pub mod aggregator;
pub mod model;
pub mod store;

pub use aggregator::FlowAggregator;
pub use model::{
    Flow, FlowBodyResponse, FlowListResponse, FlowProtocol, FlowStatus, FlowTiming, HttpMessage,
    InterceptState, SocketAddrInfo, TlsInfo, TrafficEvent,
};
pub use store::FlowStore;
