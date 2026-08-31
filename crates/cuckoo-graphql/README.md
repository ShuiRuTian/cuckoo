# cuckoo-graphql

> GraphQL 请求构造 / 内省辅助（薄层，复用 cuckoo-http）。

## 功能（M4 阶段实现）

- 不做独立协议栈，GraphQL 请求序列化成标准 `RequestBody::GraphQL{query, variables, operation_name}` 走 `cuckoo-http` 通道
- GraphQL 内省查询辅助

## 当前状态

M0 阶段仅占位，具体实现见 `spec.md` 5.3 节。

## 目录结构

```
src/
└── lib.rs    # 占位
```

## 依赖关系

- 将被 `cuckoo-service` 依赖
- 复用 `cuckoo-http` 的执行引擎
