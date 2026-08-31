# cuckoo-http

> HTTP 客户端引擎（reqwest 封装 + 精确计时）。

## 功能

- **请求执行器**：`RequestExecutor` 封装 `reqwest::Client`，支持 method / url / headers / query params / body / auth。
- **认证方式**：支持 None / Basic / Bearer / ApiKey 四种认证。
- **Body 类型**：当前支持 Raw（JSON / text / 其他），FormData / UrlEncoded / Binary 留到 M5。
- **计时数据**：粗粒度 total time 采集（DNS/TLS 精细阶段放到 M5）。
- **错误处理**：网络失败时返回结构化的 `ExecutionResult`（`success: false`），而非直接报错。

## 目录结构

```
src/
└── lib.rs    # RequestExecutor 结构体 + execute() + apply_auth() + apply_body()
```

## 依赖关系

- 被 `cuckoo-service` 依赖（`request_service::send_request` 调用）
- 依赖 `reqwest`、`cuckoo-core`（ServiceError）、`cuckoo-dto`（ExecuteRequestInput / ExecutionResult）、`cuckoo-store`（AuthConfig / RequestBody 类型）
- M1 阶段放宽 TLS 校验（`danger_accept_invalid_certs(true)`），后续由 WorkspaceSettings 控制
