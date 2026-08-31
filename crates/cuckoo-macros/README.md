# cuckoo-macros

> 提供 `#[rpc_method(METHOD, PATH)]` 属性宏。

## 功能

宏在编译期做两件事：

1. **解析函数签名**，提取完整的路由元信息（method / path / fn_name / body_type / return_type / path_params），写入 `CARGO_MANIFEST_DIR/.rpc_routes.json` 清单文件，供 `cuckoo-server/build.rs` 消费生成前端 TS 客户端和 CLI Rust 客户端。
2. **通过 `inventory::submit!`** 把元信息登记进 `cuckoo_core::rpc_registry` 的全局清单，供 server 启动期路由表打印/自检。

### 参数分类规则

基于函数签名的参数类型名自动分类：

| Rust 类型 | 分类 | 说明 |
|---|---|---|
| `DatabaseConnection` | 注入参数 | 不暴露给前端 |
| `String` | 路径参数 | 与 path 中的 `:param` 对应 |
| 其他（通常是 `*Input` 类型） | 请求 body | 序列化为 JSON body |

### 返回类型解析

- `ServiceResult<T>` → 提取 T
- `ServiceResult<Vec<T>>` → 标记为数组 `T[]`
- `ServiceResult<()>` → `void`
- 直接返回 `T`（非 ServiceResult） → 提取 T

## 目录结构

```
src/
└── lib.rs    # rpc_method 属性宏实现 + 类型解析辅助函数
```

## 依赖关系

- 是 `proc-macro` crate，编译时被 `cuckoo-service` 依赖
- 依赖 `syn`、`quote`、`proc-macro2`、`serde_json`
- 生成代码引用 `cuckoo_core::rpc_registry::inventory`
