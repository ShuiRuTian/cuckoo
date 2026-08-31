# cuckoo-mcp

> MCP Server，作为 cuckoo-server 的 HTTP 客户端（或进程内直连 service）。

## 功能（M5 阶段实现）

- 基于 `rmcp` 实现 MCP tools（`send_request` / `list_flows` / `create_rule` / `resume_intercept` 等）
- 优先支持 stdio transport（进程内可直连 `cuckoo-service` 或走 `cuckoo-server` HTTP 接口）
- Streamable HTTP transport 可延后

## 当前状态

M0 阶段只建立最小可编译骨架，具体 MCP tools 留到 `plan.md` M5.1 节集中实现。

## 目录结构

```
src/
├── main.rs     # M0 骨架：打印 help
└── tools/
    └── mod.rs  # MCP tools 定义（待 M5 实现）
```

## 依赖关系

- 计划依赖 `rmcp`、`cuckoo-server`（HTTP 客户端）或 `cuckoo-service`（进程内直连）
- 鉴权链路：启动时自动读取 `server.token` 并携带 `Authorization: Bearer` 请求头
