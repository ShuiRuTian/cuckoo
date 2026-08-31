//! `cuckoo collection` 子命令：Workspace/Folder/Request/Environment 的增删改查。
//!
//! 用法：
//! ```text
//! cuckoo collection list-workspaces
//! cuckoo collection get-workspace <id>
//! cuckoo collection create-workspace --name <name>
//! cuckoo collection list-folders <workspace_id>
//! cuckoo collection list-requests <workspace_id>
//! cuckoo collection list-environments <workspace_id>
//! ```
//!
//! 所有调用都走 `generated::cli_generated` 中自动生成的函数。

use clap::{Args, Subcommand};
use serde_json::{json, Value};

use crate::server;

#[derive(Args, Debug)]
pub struct CollectionArgs {
    #[command(subcommand)]
    pub action: CollectionAction,
}

#[derive(Subcommand, Debug)]
pub enum CollectionAction {
    /// 列出所有 Workspace
    ListWorkspaces,
    /// 获取单个 Workspace
    GetWorkspace { id: String },
    /// 创建 Workspace
    CreateWorkspace {
        #[arg(long)]
        name: String,
    },
    /// 列出 Workspace 下的 Folder
    ListFolders { workspace_id: String },
    /// 列出 Workspace 下的 Request
    ListRequests { workspace_id: String },
    /// 列出 Workspace 下的 Environment
    ListEnvironments { workspace_id: String },
}

pub async fn run(args: CollectionArgs) -> anyhow::Result<()> {
    let (base_url, token, client) = server::ensure_server(None).await?;

    match args.action {
        CollectionAction::ListWorkspaces => {
            let result =
                crate::generated::cli_generated::list_workspaces(&base_url, &token, &client)
                    .await?;
            print_json(&result);
        }
        CollectionAction::GetWorkspace { id } => {
            let result =
                crate::generated::cli_generated::get_workspace(&base_url, &token, &client, &id)
                    .await?;
            print_json(&result);
        }
        CollectionAction::CreateWorkspace { name } => {
            let body = json!({"name": name});
            let result =
                crate::generated::cli_generated::create_workspace(&base_url, &token, &client, &body)
                    .await?;
            print_json(&result);
        }
        CollectionAction::ListFolders { workspace_id } => {
            let result = crate::generated::cli_generated::list_folders(
                &base_url,
                &token,
                &client,
                &workspace_id,
            )
            .await?;
            print_json(&result);
        }
        CollectionAction::ListRequests { workspace_id } => {
            let result = crate::generated::cli_generated::list_requests(
                &base_url,
                &token,
                &client,
                &workspace_id,
            )
            .await?;
            print_json(&result);
        }
        CollectionAction::ListEnvironments { workspace_id } => {
            let result = crate::generated::cli_generated::list_environments(
                &base_url,
                &token,
                &client,
                &workspace_id,
            )
            .await?;
            print_json(&result);
        }
    }
    Ok(())
}

fn print_json(value: &Value) {
    if let Ok(pretty) = serde_json::to_string_pretty(value) {
        println!("{}", pretty);
    } else {
        println!("{}", value);
    }
}
