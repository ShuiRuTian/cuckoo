# cuckoo-ws

> WebSocket 客户端（tokio-tungstenite 封装）。

## 功能（M4 阶段实现）

- 主动连接 / 发送帧 / 接收帧的 Service 方法
- 逐帧事件通过 SSE 推送给前端
- GraphQL Subscription（`graphql-ws` 协议）识别与友好展示（可选）

## 当前状态

M0 阶段仅占位，具体实现见 `plan.md` M4 / `spec.md` 5.2 节。

## 目录结构

```
src/
└── lib.rs    # 占位
```

## 依赖关系

- 将被 `cuckoo-service` 依赖
- 计划依赖 `tokio-tungstenite`
