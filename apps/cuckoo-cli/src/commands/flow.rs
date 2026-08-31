//! `cuckoo flow` 子命令：查询/订阅抓包流量。
//!
//! 用法：
//! ```text
//! cuckoo flow list [--host <glob>] [--status <code>] [--since <time>] [--follow]
//! cuckoo flow show <flow-id> [--body request|response]
//! ```
//!
//! 注意：flow 相关的 REST 端点（`/api/flows`、`/api/flows/:id`、`/api/flows/stream`）
//! 尚未在 `cuckoo-service` 中用 `#[rpc_method]` 标注（plan.md M3 节），
//! 因此 `cli_generated.rs` 中暂时没有对应函数。
//! 等到 M3 阶段标注后，这里的调用自动可用。

use clap::{Args, Subcommand};

use crate::server;

#[derive(Args, Debug)]
pub struct FlowArgs {
    #[command(subcommand)]
    pub action: FlowAction,
}

#[derive(Subcommand, Debug)]
pub enum FlowAction {
    /// 列出历史 Flow（M3 阶段实现）
    List {
        /// 按域名过滤
        #[arg(long)]
        host: Option<String>,
        /// 按状态码过滤
        #[arg(long)]
        status: Option<u16>,
        /// 实时跟随（SSE 订阅，类似 tail -f）
        #[arg(long)]
        follow: bool,
    },
    /// 查看某个 Flow 的详情（M3 阶段实现）
    Show {
        /// Flow ID
        flow_id: String,
        /// 查看哪部分 body
        #[arg(long)]
        body: Option<String>,
    },
}

pub async fn run(args: FlowArgs) -> anyhow::Result<()> {
    let (base_url, token, client) = server::ensure_server(None).await?;

    match args.action {
        FlowAction::List { host, status, follow } => {
            // M3 阶段会有 generated::cli_generated::list_flows(...)
            // 目前用 ping 验证连通性
            let result =
                crate::generated::cli_generated::ping(&base_url, &token, &client).await?;
            println!("Server is reachable. Flow list");
            if let Some(h) = host {
                println!("  --host {}", h);
            }
            if let Some(s) = status {
                println!("  --status {}", s);
            }
            if follow {
                println!("  --follow (SSE subscription will be available in M3)");
            }
            println!("Flow endpoints will be available in M3.");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        FlowAction::Show { flow_id, body } => {
            let result =
                crate::generated::cli_generated::ping(&base_url, &token, &client).await?;
            println!("Server is reachable. Flow show: {}", flow_id);
            if let Some(b) = body {
                println!("  --body {}", b);
            }
            println!("Flow endpoints will be available in M3.");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}
