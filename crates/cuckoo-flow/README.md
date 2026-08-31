# cuckoo-flow

> Flow / Transaction 数据模型 + 序列化 + 环形缓冲存储。

## 功能（M2 阶段实现）

- `Flow` 数据结构（request / response / timing / TLS info / WS frames）
- `HttpMessage` / `FlowTiming` / `TlsInfo` / `WsFrame` 等 `#[ts(export)]` 类型定义
- 批量聚合器：内部 `mpsc` channel 收集 handler 产生的事件，16-50ms 窗口聚合后通过 `tokio::sync::broadcast` 对外暴露订阅接口
- 环形缓冲存储（大流量场景内存控制）

## 当前状态

M2 阶段已实现：Flow 数据模型定义、批量聚合器（mpsc → broadcast，16-50ms 窗口）、环形缓冲存储。

## 目录结构

```
src/
├── lib.rs         # 模块入口，重导出 Flow / FlowAggregator / FlowStore 等
├── model.rs       # Flow / HttpMessage / FlowTiming / TlsInfo / TrafficEvent 等类型定义
├── aggregator.rs  # FlowAggregator：mpsc 收集 → broadcast 批量推送
└── store.rs       # FlowStore：VecDeque 环形缓冲 + 容量上限
```

## 依赖关系

- 将被 `cuckoo-proxy` 依赖（handler 产生 Flow 事件）、`cuckoo-service` 依赖（Flow 查询方法）
- 与 `cuckoo-store` 的 `HttpMessage` / 计时结构有意共享类型定义（见 `plan.md` 风险提醒）
- 计划依赖 `tokio`（broadcast / mpsc）、`ts-rs`（TS 类型导出）
