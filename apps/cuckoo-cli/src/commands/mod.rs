//! `cuckoo` 子命令实现。
//!
//! 所有子命令均作为 `cuckoo-server` 的 HTTP/SSE 客户端运行（spec.md 7.3 节）。
//! 生成的 API 调用封装在 `generated::cli_generated` 中，本模块负责命令行参数
//! 解析 → JSON body 构造 → 调用生成函数 → 格式化输出。

pub mod send;
pub mod proxy;
pub mod server;
pub mod collection;
pub mod flow;
