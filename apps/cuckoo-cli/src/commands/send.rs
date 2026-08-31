//! `cuckoo send` 子命令：发送一次性 HTTP 请求。
//!
//! 用法：
//! ```text
//! cuckoo send <method> <url> [--header k=v]... [--body @file|-d 'json']
//! ```
//!
//! 内部构造 `SendRequestInput` 的 JSON body，调用生成的 `send_request` 函数。

use clap::Args;
use serde_json::{json, Value};

use crate::server;

#[derive(Args, Debug)]
pub struct SendArgs {
    /// HTTP 方法（GET/POST/PUT/DELETE/PATCH 等）
    pub method: String,

    /// 请求 URL
    pub url: String,

    /// 请求头，格式 `Key:Value`，可多次使用
    #[arg(long, value_name = "KEY:VALUE")]
    pub header: Vec<String>,

    /// 请求 body，直接传 JSON 字符串
    #[arg(short = 'd', long = "body", value_name = "JSON")]
    pub body: Option<String>,

    /// 从文件读取 body（`@filename` 或直接传文件路径）
    #[arg(long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
}

pub async fn run(args: SendArgs) -> anyhow::Result<()> {
    let (base_url, token, client) = server::ensure_server(None).await?;

    // 构造 headers JSON
    let headers: Vec<Value> = args
        .header
        .iter()
        .filter_map(|h| {
            let parts: Vec<&str> = h.splitn(2, ':').collect();
            if parts.len() == 2 {
                Some(json!({"key": parts[0].trim(), "value": parts[1].trim()}))
            } else {
                None
            }
        })
        .collect();

    // 构造 body
    let body_text = if let Some(ref f) = args.body_file {
        Some(std::fs::read_to_string(f)?)
    } else {
        args.body.clone()
    };

    let body_json = if let Some(ref text) = body_text {
        json!({"Raw": {"content_type": "application/json", "text": text}})
    } else {
        json!({"None": {}})
    };

    // 构造 SendRequestInput 的 ad_hoc 请求
    let input = json!({
        "ad_hoc": {
            "method": args.method.to_uppercase(),
            "url": args.url,
            "headers": headers,
            "query_params": [],
            "body": body_json,
            "auth": {"None": {}},
        }
    });

    let result = crate::generated::cli_generated::send_request(
        &base_url,
        &token,
        &client,
        &input,
    )
    .await?;

    // 格式化输出
    print_json(&result);
    Ok(())
}

/// 漂亮地打印 JSON，如果 pretty 失败则直接打印原始值。
fn print_json(value: &Value) {
    if let Ok(pretty) = serde_json::to_string_pretty(value) {
        println!("{}", pretty);
    } else {
        println!("{}", value);
    }
}
