//! 拦截规则匹配与执行引擎（`spec.md` 3.4 节 + 4.2 节，`plan.md` M3.1 节）。
//!
//! 对标 Charles 的 Breakpoints / Map Local / Map Remote / Rewrite 功能。
//!
//! 核心类型：
//! - [`RuleMatcher`]：host/path glob + method 匹配条件
//! - [`RewriteOp`]：单个改写操作（增/删/改 header、body 正则替换）
//! - [`InterceptRule`]：枚举式规则（Block/MapLocal/MapRemote/Rewrite/Breakpoint/Throttle）
//! - [`RuleOutcome`]：规则执行后的决定（短路返回 / 改写后放行 / 无匹配 / 挂起等待断点）
//! - [`RuleEngine`]：持有规则链，按顺序匹配并执行

use std::sync::Arc;

use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::handler::HttpMessage;

// ────────────────────────────────────────────────────────────────────
// 规则数据模型
// ────────────────────────────────────────────────────────────────────

/// 规则匹配条件（`spec.md` 3.4 节）。
///
/// 所有字段均为 `Option<String>`，`None` 表示"匹配任意"。
/// `host_pattern` 和 `path_pattern` 支持 glob 语法（`*` 匹配任意字符序列）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct RuleMatcher {
    /// glob 匹配 host，如 `*.example.com` 匹配 `api.example.com`
    pub host_pattern: Option<String>,
    /// glob 匹配 path，如 `/api/v1/*`
    pub path_pattern: Option<String>,
    /// 匹配 HTTP method，如 `GET`（大小写不敏感）
    pub method: Option<String>,
    /// 是否启用此规则
    pub enabled: bool,
}

impl Default for RuleMatcher {
    fn default() -> Self {
        Self {
            host_pattern: None,
            path_pattern: None,
            method: None,
            enabled: true,
        }
    }
}

impl RuleMatcher {
    /// 测试一个 HTTP 请求是否匹配此条件。
    pub fn matches(&self, req: &HttpMessage) -> bool {
        if !self.enabled {
            return false;
        }

        // method 匹配
        if let Some(ref method) = self.method {
            if !req.method.eq_ignore_ascii_case(method) {
                return false;
            }
        }

        // host 匹配
        if let Some(ref host_pattern) = self.host_pattern {
            let host = req.host().unwrap_or("");
            if !glob_match(host_pattern, host) {
                return false;
            }
        }

        // path 匹配
        if let Some(ref path_pattern) = self.path_pattern {
            let path = extract_path(&req.uri);
            if !glob_match(path_pattern, &path) {
                return false;
            }
        }

        true
    }
}

/// 单个 Rewrite 操作。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RewriteOp {
    /// 添加或覆盖一个 header
    SetHeader {
        name: String,
        value: String,
    },
    /// 删除指定 header（匹配到的全部删除）
    RemoveHeader {
        name: String,
    },
    /// 对 body 做正则替换
    ReplaceBody {
        /// 正则表达式
        pattern: String,
        /// 替换文本（支持 $1, $2 ... 反向引用）
        replacement: String,
    },
    /// 替换整个 body
    SetBody {
        content: String,
    },
}

/// 拦截规则（`spec.md` 3.4 节）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
#[serde(tag = "rule_type", rename_all = "snake_case")]
pub enum InterceptRule {
    /// 断点：命中后挂起，等待前端放行/修改/丢弃
    Breakpoint {
        match_: RuleMatcher,
        /// 是否在请求阶段断点
        on_request: bool,
        /// 是否在响应阶段断点
        on_response: bool,
    },
    /// 映射到本地：短路返回指定 body
    MapLocal {
        match_: RuleMatcher,
        /// 本地内容（直接作为 response body 返回）
        local_body: String,
        /// 可选 Content-Type
        content_type: Option<String>,
        /// 可选状态码
        status_code: Option<u16>,
    },
    /// 映射到远端：将请求转发到另一个 URL
    MapRemote {
        match_: RuleMatcher,
        /// 目标 URL 前缀（原 URL 匹配部分替换为此前缀）
        target_url: String,
    },
    /// 重写：对 header/body 做增删改
    Rewrite {
        match_: RuleMatcher,
        operations: Vec<RewriteOp>,
    },
    /// 阻断：直接返回 403 或自定义状态码
    Block {
        match_: RuleMatcher,
        /// 可选状态码（默认 403）
        status_code: Option<u16>,
    },
    /// 延迟/限速
    ThrottleOrDelay {
        match_: RuleMatcher,
        #[ts(type = "number")]
        delay_ms: u64,
        throughput_kbps: Option<u32>,
    },
}

/// 规则执行后的决定。
#[derive(Debug, Clone)]
pub enum RuleOutcome {
    /// 无规则匹配，继续正常流程
    Unchanged,
    /// 短路：直接返回此响应给客户端（Block / MapLocal）
    ShortCircuit(HttpMessage),
    /// 改写后放行（Rewrite / MapRemote）
    Rewritten(HttpMessage),
    /// 命中断点，需要挂起等待前端决策
    Pause(String, String), // (flow_id, stage)  — 实际由 InterceptRegistry 处理
}

/// 带元数据的规则（有序、可增删改查）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/lib/api/generated/")]
pub struct RuleEntry {
    /// 唯一 ID
    pub id: String,
    /// 规则名称
    pub name: String,
    /// 规则内容
    pub rule: InterceptRule,
    /// 排序键（越小越先匹配）
    pub sort_key: f64,
}

// ────────────────────────────────────────────────────────────────────
// RuleEngine
// ────────────────────────────────────────────────────────────────────

/// 规则引擎：持有有序规则链，按顺序匹配并执行。
///
/// 规则存储在 `DashMap` 中，支持运行时增删改查。
/// 匹配时按 `sort_key` 排序后顺序执行。
pub struct RuleEngine {
    rules: DashMap<String, RuleEntry>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            rules: DashMap::new(),
        }
    }

    /// 添加或更新规则。
    pub fn upsert(&self, entry: RuleEntry) {
        self.rules.insert(entry.id.clone(), entry);
    }

    /// 删除规则。
    pub fn remove(&self, id: &str) -> Option<RuleEntry> {
        self.rules.remove(id).map(|(_, v)| v)
    }

    /// 获取所有规则（按 sort_key 排序）。
    pub fn list(&self) -> Vec<RuleEntry> {
        let mut rules: Vec<RuleEntry> = self.rules.iter().map(|r| r.clone()).collect();
        rules.sort_by(|a, b| a.sort_key.partial_cmp(&b.sort_key).unwrap_or(std::cmp::Ordering::Equal));
        rules
    }

    /// 获取单条规则。
    pub fn get(&self, id: &str) -> Option<RuleEntry> {
        self.rules.get(id).map(|r| r.clone())
    }

    /// 清空所有规则。
    pub fn clear(&self) {
        self.rules.clear();
    }

    /// 对请求应用规则链。
    ///
    /// 按 sort_key 顺序匹配，返回第一个命中规则的决定。
    /// 如果没有规则匹配，返回 `Unchanged`。
    pub fn apply_request_rules(&self, req: &HttpMessage) -> RuleOutcome {
        let rules = self.list();

        for entry in rules {
            match &entry.rule {
                InterceptRule::Block { match_, status_code } => {
                    if match_.matches(req) {
                        let status = status_code.unwrap_or(403);
                        let resp = build_status_response(status);
                        return RuleOutcome::ShortCircuit(resp);
                    }
                }

                InterceptRule::MapLocal {
                    match_,
                    local_body,
                    content_type,
                    status_code,
                } => {
                    if match_.matches(req) {
                        let status = status_code.unwrap_or(200);
                        let mut resp = build_status_response(status);
                        resp.body = local_body.as_bytes().to_vec();

                        if let Some(ct) = content_type {
                            set_header(&mut resp, "Content-Type", ct);
                        }
                        if !local_body.is_empty() {
                            set_header(&mut resp, "Content-Length", &local_body.len().to_string());
                        }
                        return RuleOutcome::ShortCircuit(resp);
                    }
                }

                InterceptRule::MapRemote { match_, target_url } => {
                    if match_.matches(req) {
                        let mut rewritten = req.clone();
                        // 替换 URI 的 host:port 部分
                        rewritten.uri = replace_url_prefix(&req.uri, target_url);
                        return RuleOutcome::Rewritten(rewritten);
                    }
                }

                InterceptRule::Rewrite { match_, operations } => {
                    if match_.matches(req) {
                        let mut rewritten = req.clone();
                        for op in operations {
                            apply_rewrite_op(&mut rewritten, op);
                        }
                        return RuleOutcome::Rewritten(rewritten);
                    }
                }

                InterceptRule::Breakpoint {
                    match_,
                    on_request,
                    on_response: _,
                } => {
                    if *on_request && match_.matches(req) {
                        // 返回 Pause 指示，由 handler 层调用 InterceptRegistry
                        return RuleOutcome::Pause(String::new(), "request".to_string());
                    }
                }

                InterceptRule::ThrottleOrDelay { match_, .. } => {
                    // 延迟不在这里生效：匹配到的 ThrottleOrDelay 规则的
                    // delay_ms 总和由 `compute_request_delay` 单独计算，
                    // handler 层在转发前统一 sleep。这样不影响后续规则的
                    // 短路/改写决策（fall-through 语义）。
                    // 限速（throughput_kbps）尚未实现，前端已标注。
                    if match_.matches(req) {
                        continue;
                    }
                }
            }
        }

        RuleOutcome::Unchanged
    }

    /// 计算匹配到请求的所有 ThrottleOrDelay 规则的延迟总和。
    ///
    /// 与 `apply_request_rules` 分离：延迟与改写/短路正交，
    /// 多条延迟规则叠加时求和。handler 在转发前统一 sleep。
    pub fn compute_request_delay(&self, req: &HttpMessage) -> std::time::Duration {
        let total_ms: u64 = self
            .list()
            .into_iter()
            .filter(|entry| {
                matches!(
                    &entry.rule,
                    InterceptRule::ThrottleOrDelay { match_, .. } if match_.matches(req)
                )
            })
            .map(|entry| match entry.rule {
                InterceptRule::ThrottleOrDelay { delay_ms, .. } => delay_ms,
                _ => 0,
            })
            .sum();

        std::time::Duration::from_millis(total_ms)
    }

    /// 对响应应用规则链。
    pub fn apply_response_rules(&self, res: &HttpMessage, original_req: &HttpMessage) -> RuleOutcome {
        let rules = self.list();

        for entry in rules {
            match &entry.rule {
                InterceptRule::Rewrite { match_, operations } => {
                    if match_.matches(original_req) {
                        let mut rewritten = res.clone();
                        for op in operations {
                            apply_rewrite_op(&mut rewritten, op);
                        }
                        return RuleOutcome::Rewritten(rewritten);
                    }
                }

                InterceptRule::Breakpoint {
                    match_,
                    on_response: true,
                    ..
                } if match_.matches(original_req) => {
                    return RuleOutcome::Pause(String::new(), "response".to_string());
                }

                _ => {}
            }
        }

        RuleOutcome::Unchanged
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ────────────────────────────────────────────────────────────────────
// 辅助函数
// ────────────────────────────────────────────────────────────────────

/// 简单 glob 匹配：`*` 匹配任意字符序列（包括空），`?` 匹配单个字符。
/// 大小写不敏感（适用于 host/path 比较）。
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    glob_match_impl(&pattern, &text)
}

fn glob_match_impl(pattern: &str, text: &str) -> bool {
    // 转换为正则：* → .*，? → .，其余转义
    let mut regex_str = String::with_capacity(pattern.len() * 2);
    regex_str.push_str("(?i)^");
    for ch in pattern.chars() {
        match ch {
            '*' => regex_str.push_str(".*"),
            '?' => regex_str.push('.'),
            c if r".^${}()+|[]\\".contains(c) => {
                    regex_str.push('\\');
                    regex_str.push(c);
            }
            c => regex_str.push(c),
        }
    }
    regex_str.push('$');

    match Regex::new(&regex_str) {
        Ok(re) => re.is_match(text),
        Err(_) => false,
    }
}

/// 从 URI 中提取 path 部分。
/// `http://host:port/path?query` → `/path?query`
/// `/path` → `/path`
fn extract_path(uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        // 绝对 URI：跳过 scheme://host:port
        if let Some(slash_pos) = uri[8..].find('/') {
            return uri[8 + slash_pos..].to_string();
        }
        return "/".to_string();
    }
    uri.to_string()
}

/// 替换 URL 前缀（MapRemote 用）。
fn replace_url_prefix(original_uri: &str, target_url: &str) -> String {
    // 从原始 URI 提取 path+query
    let path = extract_path(original_uri);
    // 拼接 target_url + path（target_url 末尾的 '/' 去重，避免双斜杠）
    let base = target_url.strip_suffix('/').unwrap_or(target_url);
    format!("{}{}", base, path)
}

/// 构造一个简单状态响应。
fn build_status_response(status: u16) -> HttpMessage {
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };

    HttpMessage {
        method: String::new(),
        uri: String::new(),
        version: "HTTP/1.1".to_string(),
        headers: vec![
            (":status".to_string(), format!("{status} {reason}")),
            ("Content-Length".to_string(), "0".to_string()),
        ],
        body: Vec::new(),
    }
}

/// 设置/覆盖 header。
fn set_header(msg: &mut HttpMessage, name: &str, value: &str) {
    // 移除已有的同名 header
    msg.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    msg.headers.push((name.to_string(), value.to_string()));
}

/// 应用单个 Rewrite 操作到 HTTP 消息。
fn apply_rewrite_op(msg: &mut HttpMessage, op: &RewriteOp) {
    match op {
        RewriteOp::SetHeader { name, value } => {
            set_header(msg, name, value);
        }
        RewriteOp::RemoveHeader { name } => {
            msg.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        }
        RewriteOp::ReplaceBody {
            pattern,
            replacement,
        } => {
            let body_str = String::from_utf8_lossy(&msg.body);
            if let Ok(re) = Regex::new(pattern) {
                let new_body = re.replace_all(&body_str, replacement.as_str());
                msg.body = new_body.into_owned().into_bytes();
                set_header(msg, "Content-Length", &msg.body.len().to_string());
            }
        }
        RewriteOp::SetBody { content } => {
            msg.body = content.as_bytes().to_vec();
            set_header(msg, "Content-Length", &content.len().to_string());
        }
    }
}

/// 共享 RuleEngine 的类型别名。
pub type SharedRuleEngine = Arc<RuleEngine>;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(method: &str, uri: &str, host: &str) -> HttpMessage {
        HttpMessage {
            method: method.to_string(),
            uri: uri.to_string(),
            version: "HTTP/1.1".to_string(),
            headers: vec![("Host".to_string(), host.to_string())],
            body: Vec::new(),
        }
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.example.com", "api.example.com"));
        assert!(glob_match("*.example.com", "sub.api.example.com"));
        assert!(!glob_match("*.example.com", "example.org"));
        assert!(glob_match("/api/v1/*", "/api/v1/users"));
        assert!(glob_match("/api/v1/*", "/api/v1/users/123"));
        assert!(!glob_match("/api/v1/*", "/api/v2/users"));
    }

    #[test]
    fn test_rule_matcher() {
        let matcher = RuleMatcher {
            host_pattern: Some("*.example.com".to_string()),
            path_pattern: Some("/api/*".to_string()),
            method: Some("GET".to_string()),
            enabled: true,
        };

        let req = make_request("GET", "http://api.example.com/api/users", "api.example.com");
        assert!(matcher.matches(&req));

        let req2 = make_request("POST", "http://api.example.com/api/users", "api.example.com");
        assert!(!matcher.matches(&req2)); // method 不匹配

        let req3 = make_request("GET", "http://api.example.com/v2/users", "api.example.com");
        assert!(!matcher.matches(&req3)); // path 不匹配
    }

    #[test]
    fn test_block_rule() {
        let engine = RuleEngine::new();
        engine.upsert(RuleEntry {
            id: "r1".to_string(),
            name: "Block example.com".to_string(),
            rule: InterceptRule::Block {
                match_: RuleMatcher {
                    host_pattern: Some("*.example.com".to_string()),
                    path_pattern: None,
                    method: None,
                    enabled: true,
                },
                status_code: Some(403),
            },
            sort_key: 1.0,
        });

        let req = make_request("GET", "http://api.example.com/api", "api.example.com");
        match engine.apply_request_rules(&req) {
            RuleOutcome::ShortCircuit(resp) => {
                assert!(resp.headers.iter().any(|(k, v)| k == ":status" && v.contains("403")));
            }
            _ => panic!("expected ShortCircuit"),
        }
    }

    #[test]
    fn test_rewrite_header() {
        let engine = RuleEngine::new();
        engine.upsert(RuleEntry {
            id: "r1".to_string(),
            name: "Add X-Custom header".to_string(),
            rule: InterceptRule::Rewrite {
                match_: RuleMatcher {
                    host_pattern: Some("api.example.com".to_string()),
                    path_pattern: None,
                    method: None,
                    enabled: true,
                },
                operations: vec![RewriteOp::SetHeader {
                    name: "X-Custom".to_string(),
                    value: "cuckoo".to_string(),
                }],
            },
            sort_key: 1.0,
        });

        let req = make_request("GET", "http://api.example.com/api", "api.example.com");
        match engine.apply_request_rules(&req) {
            RuleOutcome::Rewritten(rewritten) => {
                assert!(rewritten.headers.iter().any(|(k, v)| k == "X-Custom" && v == "cuckoo"));
            }
            _ => panic!("expected Rewritten"),
        }
    }

    #[test]
    fn test_map_local() {
        let engine = RuleEngine::new();
        engine.upsert(RuleEntry {
            id: "r1".to_string(),
            name: "Map Local".to_string(),
            rule: InterceptRule::MapLocal {
                match_: RuleMatcher {
                    host_pattern: Some("api.example.com".to_string()),
                    path_pattern: Some("/fake".to_string()),
                    method: None,
                    enabled: true,
                },
                local_body: r#"{"mock":true}"#.to_string(),
                content_type: Some("application/json".to_string()),
                status_code: Some(200),
            },
            sort_key: 1.0,
        });

        let req = make_request("GET", "http://api.example.com/fake", "api.example.com");
        match engine.apply_request_rules(&req) {
            RuleOutcome::ShortCircuit(resp) => {
                assert_eq!(resp.body, br#"{"mock":true}"#);
                assert!(resp.headers.iter().any(|(k, v)| k == "Content-Type" && v == "application/json"));
            }
            _ => panic!("expected ShortCircuit"),
        }
    }

    #[test]
    fn test_breakpoint_on_request() {
        let engine = RuleEngine::new();
        engine.upsert(RuleEntry {
            id: "r1".to_string(),
            name: "Breakpoint".to_string(),
            rule: InterceptRule::Breakpoint {
                match_: RuleMatcher {
                    host_pattern: Some("api.example.com".to_string()),
                    path_pattern: None,
                    method: None,
                    enabled: true,
                },
                on_request: true,
                on_response: false,
            },
            sort_key: 1.0,
        });

        let req = make_request("GET", "http://api.example.com/api", "api.example.com");
        match engine.apply_request_rules(&req) {
            RuleOutcome::Pause(_, stage) => {
                assert_eq!(stage, "request");
            }
            _ => panic!("expected Pause"),
        }
    }
}
