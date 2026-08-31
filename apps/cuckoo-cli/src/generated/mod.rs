//! 自动生成的 CLI 客户端代码。
//!
//! 本目录下的 `cli_generated.rs` 由 `cuckoo-server/build.rs` 在编译期
//! 从 `#[rpc_method]` 宏收集的路由元信息自动生成，包含所有 REST 端点
//! 的 Rust 调用封装。
//!
//! **不要手动编辑此目录下的文件。**
//!
//! 生成代码中的每个函数签名统一为：
//! ```text
//! pub async fn <fn_name>(
//!     base_url: &str,
//!     token: &str,
//!     client: &reqwest::Client,
//!     [path_params: &str, ...]
//!     [body: &serde_json::Value]
//! ) -> anyhow::Result<serde_json::Value>
//! ```
//!
//! CLI 子命令通过 `crate::generated::cli_generated::xxx()` 调用对应的端点。

#[allow(clippy::all, unused_imports, dead_code)]
pub mod cli_generated;
