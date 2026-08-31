# cuckoo-templates

> 变量插值引擎（`{{var}}` 渲染，环境变量解析链）。

## 功能

- **变量上下文**：`VariableContext` 包含从 Environment / Workspace 等来源收集的变量键值对。
- **插值渲染**：`{{variable}}` 语法，支持空格容错（`{{ var }}` = `{{var}}`）。
- **多目标渲染**：URL / Headers / Query Params / Body Text 均可渲染。
- **缺失变量保留**：未找到的变量保持原样 `{{unknown}}`，不报错。

### 解析链顺序（完整版见 spec.md 5.1 节）

```
请求级 override → Folder 继承 → Environment → Workspace 全局变量
```

> M1 阶段先做 Environment 级变量替换，不做 Folder 继承。

## 目录结构

```
src/
└── lib.rs    # VariableContext + render() + render_headers/query_params/url/body_text
```

## 依赖关系

- 被 `cuckoo-service` 依赖（`request_service::send_request` 前渲染变量）
- 依赖 `cuckoo-store`（EnvVariable / KeyValueEntry / HeaderEntry 类型）、`regex`、`once_cell`
- 包含单元测试：基本插值、缺失变量保留、空格容错、多变量
