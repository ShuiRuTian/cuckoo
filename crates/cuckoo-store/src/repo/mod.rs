//! CRUD 仓库层（纯数据层逻辑，不含 `#[rpc_method]` 包装）。
//!
//! 每个 Entity 的 CRUD 操作集中在一个子模块里。

pub mod workspace_repo;
pub mod folder_repo;
pub mod request_repo;
pub mod environment_repo;
