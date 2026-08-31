//! `cuckoo proxy` 子命令：代理生命周期管理。
//!
//! 用法：
//! ```text
//! cuckoo proxy start [--port 8899] [--system-proxy]
//! cuckoo proxy stop
//! cuckoo proxy status
//! ```
//!
//! 注意：当前 `cuckoo-service` 尚未实现 proxy 相关方法（plan.md M3 节），
//! 这里的命令在生成的 `cli_generated.rs` 中没有对应函数时会编译报错。
//! 等到 M3 阶段 `#[rpc_method]` 标注的 proxy 方法出现后，这里自动可用。
//! 目前用 `ping` 替代 `status` 做连通性验证。

use clap::{Args, Subcommand};

use crate::server;

#[derive(Args, Debug)]
pub struct ProxyArgs {
    #[command(subcommand)]
    pub action: ProxyAction,
}

#[derive(Subcommand, Debug)]
pub enum ProxyAction {
    /// 启动代理（M3 阶段实现）
    Start {
        /// 代理监听端口
        #[arg(long, default_value = "8899")]
        port: u16,
        /// 同时设置系统代理
        #[arg(long)]
        system_proxy: bool,
    },
    /// 停止代理（M3 阶段实现）
    Stop,
    /// 查看代理状态（M3 阶段实现）
    Status,
}

pub async fn run(args: ProxyArgs) -> anyhow::Result<()> {
    match args.action {
        ProxyAction::Start { port, system_proxy } => {
            let (base_url, token, client) = server::ensure_server(None).await?;
            // M3 阶段会有 generated::cli_generated::start_proxy(...)
            // 目前只是验证连通性
            let result = crate::generated::cli_generated::ping(&base_url, &token, &client).await?;
            println!("Server is reachable. Proxy start (port={}, system_proxy={}) will be available in M3.", port, system_proxy);
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ProxyAction::Stop => {
            let (base_url, token, client) = server::ensure_server(None).await?;
            let result = crate::generated::cli_generated::ping(&base_url, &token, &client).await?;
            println!("Server is reachable. Proxy stop will be available in M3.");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ProxyAction::Status => {
            let (base_url, token, client) = server::ensure_server(None).await?;
            let result = crate::generated::cli_generated::ping(&base_url, &token, &client).await?;
            println!("Server is reachable. Proxy status will be available in M3.");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}
