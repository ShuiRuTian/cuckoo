//! `cuckoo-cli`：命令行工具，作为 `cuckoo-server` 的 HTTP/SSE 客户端
//! （`spec.md` 2.1 节、7.3 节）。
//!
//! 子命令通过 `#[rpc_method]` 宏生成的 `cli_generated.rs` 调用 Server 的 REST 端点。
//! 如果本地没有运行中的 Server，CLI 会自动拉起一个 headless `cuckoo-server` 子进程
//! （spec.md 2.2 节第 4 点）。

mod commands;
mod generated;
mod server;

use clap::{Parser, Subcommand};

use commands::{collection, flow, proxy, send, server as server_cmd};

#[derive(Parser, Debug)]
#[command(
    name = "cuckoo",
    about = "Cuckoo CLI —— cuckoo-server 的命令行客户端",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 发送一次性 HTTP 请求
    Send(send::SendArgs),

    /// 代理生命周期管理（M3 阶段完整实现）
    Proxy(proxy::ProxyArgs),

    /// Collection/Workspace/Folder/Request/Environment CRUD
    Collection(collection::CollectionArgs),

    /// 抓包流量查询（M3 阶段完整实现）
    Flow(flow::FlowArgs),

    /// 显式拉起本地 cuckoo-server
    Server(server_cmd::ServerArgs),

    /// 打印版本信息
    Version,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async move {
        match cli.command {
            Some(Commands::Send(args)) => send::run(args).await,
            Some(Commands::Proxy(args)) => proxy::run(args).await,
            Some(Commands::Collection(args)) => collection::run(args).await,
            Some(Commands::Flow(args)) => flow::run(args).await,
            Some(Commands::Server(args)) => server_cmd::run(args).await,
            Some(Commands::Version) | None => {
                println!(
                    "cuckoo-cli {} (auto-generated RPC client)",
                    env!("CARGO_PKG_VERSION")
                );
                Ok(())
            }
        }
    })
}
