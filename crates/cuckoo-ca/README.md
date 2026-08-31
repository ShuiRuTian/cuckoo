# cuckoo-ca

> 证书体系（根 CA 生成 / 持久化 / 安装引导，叶子证书签发）。

## 功能（M2 阶段实现）

- 应用首次启动生成根 CA（`rcgen`），持久化到应用数据目录
- 基于 SNI 现场签发叶子证书（`DashMap` / `moka` 缓存）
- CA 证书导出方法（`export_ca_cert()`，通过 `cuckoo-service` 暴露为 REST 端点）
- 分平台安装说明文案

## 当前状态

M2 阶段已实现：根 CA 生成/持久化、叶子证书现场签发、`DashMap` 缓存。

## 目录结构

```
src/
├── lib.rs           # 模块入口，重导出 CaAuthority / CaError
├── authority.rs     # CaAuthority：load_or_create() / get_or_issue_server_config() / export_ca_cert_pem()
└── error.rs         # CaError 错误类型
```

## 依赖关系

- 将被 `cuckoo-proxy` 依赖（TLS 动态签发）
- 将被 `cuckoo-service` 依赖（证书导出方法）
- 计划依赖 `rcgen`、`tokio-rustls`、`dashmap` / `moka`
