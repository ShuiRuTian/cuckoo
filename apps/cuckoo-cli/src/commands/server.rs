//! `cuckoo server` 子命令：显式拉起本地 Server。
//!
//! 用法：
//! ```text
//! cuckoo server start [--headless] [--port 4173]
//! ```

use clap::{Args, Subcommand};
use std::process::Command;

#[derive(Args, Debug)]
pub struct ServerArgs {
    #[command(subcommand)]
    pub action: ServerAction,
}

#[derive(Subcommand, Debug)]
pub enum ServerAction {
    /// 启动本地 cuckoo-server（前台运行，Ctrl+C 退出）
    Start {
        /// 以无 GUI 的独立进程方式运行
        #[arg(long)]
        headless: bool,

        /// 监听端口
        #[arg(long)]
        port: Option<u16>,
    },
}

pub async fn run(args: ServerArgs) -> anyhow::Result<()> {
    match args.action {
        ServerAction::Start { headless, port } => {
            // 尝试找到 cuckoo-server 二进制
            let bin = find_server_binary()?;

            let mut cmd = Command::new(&bin);
            if headless {
                cmd.arg("--headless");
            }
            if let Some(p) = port {
                cmd.arg("--port").arg(p.to_string());
            }

            // 前台运行，让用户直接看到 Server 日志
            let status = cmd.status().map_err(|e| {
                anyhow::anyhow!("failed to start cuckoo-server ({}): {}", bin, e)
            })?;

            if !status.success() {
                anyhow::bail!("cuckoo-server exited with non-zero status");
            }
        }
    }
    Ok(())
}

/// 尝试推导 `cuckoo-server` 二进制路径。
fn find_server_binary() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine exe parent dir"))?;

    let candidate = dir.join("cuckoo-server");
    if candidate.exists() {
        return Ok(candidate.to_string_lossy().to_string());
    }

    Ok("cuckoo-server".to_string())
}
