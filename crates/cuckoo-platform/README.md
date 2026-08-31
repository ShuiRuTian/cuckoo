# cuckoo-platform

> 系统集成（代理设置、CA 信任安装，分平台实现）。

## 功能（M2 阶段实现）

- macOS 的 `networksetup` 分支（优先实现，主要开发环境）
- Windows / Linux 分支（先留 TODO stub，M5 补齐）
- "一键开启系统代理"开关
- 应用退出时自动恢复系统代理设置的 hook

## 当前状态

M2 阶段已实现：macOS `networksetup` 分支完成，Windows/Linux 留 stub（M5 补齐）。

## 目录结构

```
src/
├── lib.rs       # SystemProxyManager trait + create_proxy_manager() + ProxySnapshot
├── macos.rs     # MacOsProxyManager：通过 networksetup 设置/清除系统代理
└── stub.rs      # StubProxyManager：Windows/Linux 占位实现
```

## 依赖关系

- 将被 `cuckoo-desktop`（退出时恢复代理）和 `cuckoo-service`（系统代理设置方法）依赖
- macOS 分支将调用 `networksetup` 命令行工具
