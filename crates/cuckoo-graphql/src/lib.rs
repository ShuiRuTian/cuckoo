//! `cuckoo-graphql`：GraphQL 请求构造/内省辅助（薄层，复用 `cuckoo-http`）。
//!
//! M0 阶段仅占位，具体实现见 `spec.md` 5.3 节：不做独立协议栈，GraphQL 请求
//! 序列化成标准 `RequestBody::GraphQL{query, variables, operation_name}` 走
//! `cuckoo-http` 通道。

#[allow(dead_code)]
const _PLACEHOLDER: () = ();
