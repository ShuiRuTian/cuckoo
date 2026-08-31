//! `cuckoo-mcp`：MCP Server，作为 `cuckoo-server` 的 HTTP 客户端（或进程内直连
//! service，见 `spec.md` 2.1 节、7.4 节）。
//!
//! M0 阶段只建立最小可编译骨架（`main.rs` 打印 help 即可），具体 MCP tools
//! （`send_request`/`list_flows`/`create_rule`/`resume_intercept` 等）留到
//! `plan.md` M5.1 节基于 `rmcp` 集中实现。

mod tools;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!(
        "cuckoo-mcp {} (M0 骨架，MCP tools 待 M5 基于 rmcp 实现)",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}
