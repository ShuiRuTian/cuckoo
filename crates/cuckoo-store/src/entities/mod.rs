//! Sea-ORM Entity 模块（`spec.md` 3.2 节）。
//!
//! 四个核心 Entity：Workspace / Folder / HttpRequestDef / Environment，
//! 复合结构（`Vec<HeaderEntry>` 等）以 JSON 列存储。

pub mod workspace;
pub mod folder;
pub mod http_request_def;
pub mod environment;

pub use workspace::Entity as WorkspaceEntity;
pub use folder::Entity as FolderEntity;
pub use http_request_def::Entity as HttpRequestDefEntity;
pub use environment::Entity as EnvironmentEntity;
