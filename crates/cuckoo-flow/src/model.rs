//! Flow 数据模型定义（`spec.md` 3.3 节，`plan.md` M2.3 节）。
//!
//! M2 精简版：聚焦 request/response/timing，不含 TLS 详情/WS 帧
//! （后续 M4/M5 阶段补齐）。
//!
//! 所有类型通过 `ts-rs` 导出 TypeScript 定义，与前端共享单一真源。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 抓包协议类型。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(rename_all = "snake_case")]
pub enum FlowProtocol {
    Http1,
    Http2,
    WebSocket,
    /// 占位，v1 不支持 MITM 拦截
    Http3,
}

/// Flow 状态。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(rename_all = "snake_case")]
pub enum FlowStatus {
    /// 请求已发出，等待响应
    Pending,
    /// 请求-响应完整
    Complete,
    /// 出错（连接失败、解析错误等）
    Error,
    /// 命中断点，等待用户放行/修改/丢弃
    Intercepted,
}

/// 拦截状态。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(rename_all = "snake_case")]
pub enum InterceptState {
    NotIntercepted,
    /// 挂起在指定阶段
    Paused {
        /// "request" 或 "response"
        stage: String,
    },
    Resumed,
}

/// Socket 地址信息（简化版，避免 `SocketAddr` 不可序列化的问题）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct SocketAddrInfo {
    pub ip: String,
    pub port: u16,
}

/// HTTP 消息（Flow 内部的 request/response 共用结构）。
///
/// M2 精简版：body 内联在消息中，后续大 body 改为 `BodyRef` 惰性拉取。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct HttpMessage {
    /// 如 "GET /api/users HTTP/1.1" 或 "HTTP/1.1 200 OK"
    pub start_line: String,
    /// 方法（仅 request 有，response 为空字符串）
    pub method: String,
    /// URI（仅 request 有，response 为空字符串）
    pub uri: String,
    /// HTTP 版本（如 "HTTP/1.1"）
    pub version: String,
    /// 状态码（仅 response 有，request 为 None）
    pub status_code: Option<u16>,
    /// 保序 header 列表，允许重复 key
    pub headers: Vec<(String, String)>,
    /// 原始 header 块文本，用于精确还原
    pub headers_raw: Option<String>,
    /// Body 内容（M2 阶段内联；后续改为 BodyRef 惰性拉取）
    pub body: Vec<u8>,
    /// Body 大小（可能与 body.len() 不同，如果被截断）
    pub body_size: usize,
    /// Body 是否被截断（超过大小上限时）
    pub body_truncated: bool,
}

impl HttpMessage {
    /// 获取第一个匹配 header 的值。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Flow 计时信息（参考 CDP ResourceTiming + spec.md 3.3 节）。
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct FlowTiming {
    /// 开始时间（epoch 毫秒）
    #[ts(type = "number")]
    pub start_time: i64,
    /// DNS 解析开始
    #[ts(type = "number | null")]
    pub dns_start: Option<i64>,
    #[ts(type = "number | null")]
    pub dns_end: Option<i64>,
    /// TCP 连接
    #[ts(type = "number | null")]
    pub connect_start: Option<i64>,
    #[ts(type = "number | null")]
    pub connect_end: Option<i64>,
    /// TLS 握手
    #[ts(type = "number | null")]
    pub tls_start: Option<i64>,
    #[ts(type = "number | null")]
    pub tls_end: Option<i64>,
    /// 请求发送
    #[ts(type = "number | null")]
    pub send_start: Option<i64>,
    #[ts(type = "number | null")]
    pub send_end: Option<i64>,
    /// Time To First Byte
    #[ts(type = "number | null")]
    pub ttfb: Option<i64>,
    /// 结束时间
    #[ts(type = "number | null")]
    pub end_time: Option<i64>,
}

/// TLS 连接信息（M2 精简版，后续补齐证书链详情）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct TlsInfo {
    pub version: String,
    pub cipher: String,
    pub sni: Option<String>,
    pub alpn: Option<String>,
}

/// 完整的 Flow 记录（spec.md 3.3 节）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct Flow {
    /// ULID，保证时间有序
    pub id: String,
    pub protocol: FlowProtocol,
    pub client_addr: Option<SocketAddrInfo>,
    pub server_addr: Option<SocketAddrInfo>,
    pub request: HttpMessage,
    pub response: Option<HttpMessage>,
    pub timing: FlowTiming,
    pub tls: Option<TlsInfo>,
    pub status: FlowStatus,
    pub error: Option<String>,
    pub intercept: InterceptState,
    pub tags: Vec<String>,
}

/// `GET /api/flows` 的列表响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct FlowListResponse {
    pub flows: Vec<Flow>,
    /// 存储中的 Flow 总数（不受分页影响）
    pub total: usize,
}

/// `GET /api/flows/{id}/body` 的 body 响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct FlowBodyResponse {
    /// UTF-8 文本原文；二进制内容为 base64 编码（见 `binary` 字段）
    pub body: String,
    #[ts(type = "number")]
    pub body_size: usize,
    pub body_truncated: bool,
    pub content_type: Option<String>,
    /// true 表示 `body` 是 base64 编码的二进制内容，前端需先解码
    pub binary: bool,
}

/// SSE 推送给前端的事件类型。
///
/// 对应 `spec.md` 6.3 节的 `TrafficEvent`，前端 `EventSource` 订阅
/// `flow.batch` 事件后解析为 `TrafficEvent[]`。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrafficEvent {
    /// 新 Flow 开始（收到请求头）
    FlowStarted {
        #[serde(flatten)]
        flow: Flow,
    },
    /// Flow 完成（收到响应）
    FlowComplete {
        flow_id: String,
        #[serde(flatten)]
        flow: Flow,
    },
    /// Flow 出错
    FlowError {
        flow_id: String,
        error: String,
    },
    /// Flow 命中断点
    FlowIntercepted {
        flow_id: String,
        stage: String,
    },
}
