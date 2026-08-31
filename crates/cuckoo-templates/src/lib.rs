//! `cuckoo-templates`：变量插值引擎（`{{var}}` 渲染，环境变量解析链）。
//!
//! M1 阶段先做 Environment 级变量替换，不做 Folder 继承。
//! 解析链顺序（完整版见 `spec.md` 5.1 节）：
//! 请求级 override → Folder 继承 → Environment → Workspace 全局变量。

use std::collections::HashMap;

use cuckoo_store::entities::environment::EnvVariable;
use cuckoo_store::entities::http_request_def::KeyValueEntry;
use cuckoo_store::entities::workspace::HeaderEntry;
use once_cell::sync::Lazy;
use regex::Regex;

static VAR_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{(\s*[\w.-]+\s*)\}\}").unwrap());

/// 变量解析上下文：包含从 Environment、Workspace 等来源收集的变量键值对。
#[derive(Debug, Clone, Default)]
pub struct VariableContext {
    pub variables: HashMap<String, String>,
}

impl VariableContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 Environment 的变量列表构建上下文。
    pub fn from_env_variables(variables: &[EnvVariable]) -> Self {
        let mut ctx = Self::new();
        for var in variables {
            if var.enabled {
                ctx.variables.insert(var.key.clone(), var.value.clone());
            }
        }
        ctx
    }

    /// 从 JSON（EnvVariable 列表）构建上下文。
    pub fn from_env_json(json: &serde_json::Value) -> Self {
        let vars: Vec<EnvVariable> = serde_json::from_slice(
            serde_json::to_string(json).unwrap_or_default().as_bytes(),
        )
        .unwrap_or_default();
        Self::from_env_variables(&vars)
    }

    /// 添加变量。
    pub fn insert(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    /// 对字符串进行 `{{variable}}` 插值渲染。
    pub fn render(&self, text: &str) -> String {
        VAR_REGEX
            .replace_all(text, |caps: &regex::Captures| {
                let key = caps.get(1).unwrap().as_str().trim();
                self.variables
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| format!("{{{{{key}}}}}"))
            })
            .to_string()
    }

    /// 对 Header 列表渲染变量。
    pub fn render_headers(&self, headers: &mut Vec<HeaderEntry>) {
        for h in headers {
            h.name = self.render(&h.name);
            h.value = self.render(&h.value);
        }
    }

    /// 对 QueryParam 列表渲染变量。
    pub fn render_query_params(&self, params: &mut Vec<KeyValueEntry>) {
        for p in params {
            p.key = self.render(&p.key);
            p.value = self.render(&p.value);
        }
    }

    /// 对 URL 渲染变量。
    pub fn render_url(&self, url: &str) -> String {
        self.render(url)
    }

    /// 对 body 文本渲染变量（仅 Raw body 类型）。
    pub fn render_body_text(&self, text: &str) -> String {
        self.render(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_interpolation() {
        let mut ctx = VariableContext::new();
        ctx.insert("baseUrl", "https://httpbin.org");
        let result = ctx.render("{{baseUrl}}/get");
        assert_eq!(result, "https://httpbin.org/get");
    }

    #[test]
    fn test_missing_variable_preserved() {
        let ctx = VariableContext::new();
        let result = ctx.render("{{unknown}}/path");
        assert_eq!(result, "{{unknown}}/path");
    }

    #[test]
    fn test_whitespace_in_var() {
        let mut ctx = VariableContext::new();
        ctx.insert("host", "example.com");
        assert_eq!(ctx.render("{{ host }}"), "example.com");
        assert_eq!(ctx.render("{{host}}"), "example.com");
    }

    #[test]
    fn test_multiple_vars() {
        let mut ctx = VariableContext::new();
        ctx.insert("baseUrl", "https://api.test");
        ctx.insert("path", "users");
        ctx.insert("id", "42");
        assert_eq!(
            ctx.render("{{baseUrl}}/{{path}}/{{id}}"),
            "https://api.test/users/42"
        );
    }
}
