//! `cuckoo-server` 独立运行入口（`cuckoo-server --headless --port 4173`）。
//!
//! 供"只想用命令行/AI 操作、不需要 GUI"的场景使用（`spec.md` 2.2 节第 3 点）；
//! 也是 `cuckoo-cli` 在未检测到本地 Server 时可以自动拉起的子进程目标
//! （`plan.md` M5.1 节）。

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cuckoo-server", about = "Cuckoo 本地 HTTP+SSE Server")]
struct Cli {
    /// 以无 GUI 的独立进程方式运行（M0 阶段该 flag 暂无实际行为差异，
    /// 独立二进制本身就不含任何 GUI 代码；保留参数是为了让
    /// `cuckoo-cli`/文档里的调用方式提前稳定下来）。
    #[arg(long)]
    headless: bool,

    /// 监听端口，不指定则由操作系统分配一个空闲端口。
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cuckoo_server=info,tower_http=info".into()),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!(headless = cli.headless, port = ?cli.port, "starting cuckoo-server");

    let handle = cuckoo_server::spawn_server(cli.port).await?;
    tracing::info!(addr = %handle.addr, "cuckoo-server ready; token file: {:?}", cuckoo_server::auth::token_file_path());

    handle.join_handle.await?;
    Ok(())
}
