//! `build.rs`：从 `#[rpc_method]` 宏生成的清单自动生成多端客户端代码。
//!
//! 工作流程：
//! 1. `#[rpc_method]` 宏在编译 `cuckoo-service` 时，将每个标注方法的路由元信息
//!    （method/path/fn_name/body_type/return_type/path_params）写入 crate 根目录的
//!    `.rpc_routes.json` 清单文件。
//! 2. 本 `build.rs` 在编译 `cuckoo-server` 时，扫描 `crates/` 目录下所有
//!    `.rpc_routes.json` 文件，合并成一个完整的路由清单。
//! 3. 根据清单自动生成：
//!    - `src/lib/api/generated/api.ts`（前端 TS fetch 函数）
//!    - `src/lib/api/generated/types.ts` + `index.ts`
//!    - `apps/cuckoo-cli/src/generated/cli_generated.rs`（CLI Rust 客户端封装）
//!
//! CLI 侧生成的代码使用 `serde_json::Value` 而非强类型 DTO，
//! 因为 `cuckoo-cli` 不依赖 `cuckoo-service`/`cuckoo-dto`（spec.md 2.2 节），
//! 只关心 JSON 的序列化/反序列化。

use std::fs;
use std::path::{Path, PathBuf};

/// 一条 API 路由的元信息（从宏生成的 JSON 反序列化）。
#[derive(Debug, Clone, serde::Deserialize)]
struct RpcRoute {
    method: String,
    path: String,
    /// Rust 函数名（snake_case）
    fn_name: String,
    /// TypeScript 函数名（camelCase）
    ts_fn_name: String,
    /// 请求 body 的 TypeScript 类型（如有）
    body_type: Option<String>,
    /// 返回类型的 TypeScript 类型
    return_type: String,
    /// 路径参数名列表
    path_params: Vec<String>,
    /// 从 URL path 中提取的参数名（用于交叉验证）
    #[allow(dead_code)]
    path_param_names: Vec<String>,
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent() // apps/
        .and_then(|p| p.parent()) // workspace root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let output_dir = workspace_root.join("src/lib/api/generated");
    fs::create_dir_all(&output_dir).ok();

    // ── 收集所有 rpc_routes.json 文件 ──
    let routes = collect_all_routes(&workspace_root);

    // ── 生成 api.ts ──
    generate_api_ts(&output_dir, &routes);

    // ── 生成 types.ts（合并 ts-rs 生成的类型文件）──
    generate_types_ts(&output_dir);

    // ── 生成 index.ts ──
    generate_index_ts(&output_dir);

    // ── 生成 CLI Rust 客户端 ──
    let cli_generated_dir = workspace_root.join("apps/cuckoo-cli/src/generated");
    fs::create_dir_all(&cli_generated_dir).ok();
    generate_cli_rs(&cli_generated_dir, &routes);

    // 清理旧版单文件清单（已迁移到 .rpc_routes/ 目录方案）
    let legacy = workspace_root.join("crates/cuckoo-service/.rpc_routes.json");
    if legacy.exists() {
        let _ = fs::remove_file(&legacy);
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", output_dir.display());
    println!("cargo:rerun-if-changed={}", cli_generated_dir.join("cli_generated.rs").display());
    // 路由清单目录变化时重新生成
    let service_routes_dir = workspace_root.join("crates/cuckoo-service/.rpc_routes");
    println!("cargo:rerun-if-changed={}", service_routes_dir.display());
}

/// 扫描 `crates/` 目录，收集所有 `.rpc_routes/*.json` 路由清单文件。
///
/// `#[rpc_method]` 宏在编译 `cuckoo-service` 等 crate 时，将每个方法的路由
/// 元信息写入该 crate 根目录下的 `.rpc_routes/<fn_name>.json`（每方法一个文件，
/// 幂等且无并发竞态）。`build.rs` 递归扫描所有 crate 目录来收集。
fn collect_all_routes(workspace_root: &Path) -> Vec<RpcRoute> {
    let crates_dir = workspace_root.join("crates");
    let mut all_routes: Vec<RpcRoute> = Vec::new();

    fn scan_dir(dir: &std::path::Path, routes: &mut Vec<RpcRoute>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 跳过 target 等非源码目录
                    if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                        continue;
                    }
                    scan_dir(&path, routes);
                } else if path.extension().map(|e| e == "json").unwrap_or(false) {
                    // 仅处理位于 .rpc_routes 目录下的 json 文件
                    if path
                        .parent()
                        .and_then(|p| p.file_name().and_then(|n| n.to_str()))
                        != Some(".rpc_routes")
                    {
                        continue;
                    }
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(route) = serde_json::from_str::<RpcRoute>(&content) {
                            // 去重：同一个 (method, path) 组合只保留一条
                            if !routes
                                .iter()
                                .any(|r| r.method == route.method && r.path == route.path)
                            {
                                routes.push(route);
                            }
                        }
                    }
                }
            }
        }
    }

    if crates_dir.exists() {
        scan_dir(&crates_dir, &mut all_routes);
    }

    // 按路径排序，让生成的 api.ts 更有序
    all_routes.sort_by(|a, b| {
        a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method))
    });

    all_routes
}

/// 扫描 generated 目录下所有 ts-rs 生成的类型名（排除本构建产物自身）。
///
/// 返回按字典序排序的类型名列表，保证生成产物确定性。
fn scan_exported_types(output_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(output_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.extension().map(|e| e == "ts").unwrap_or(false)
                && !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("api.ts") | Some("types.ts") | Some("index.ts")
                )
        })
        .filter_map(|p| {
            p.file_stem()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// 生成 `api.ts`：强类型 fetch 函数 + 类型 re-export。
fn generate_api_ts(output_dir: &Path, routes: &[RpcRoute]) {
    let mut code = String::new();

    code.push_str("// This file is auto-generated by `cuckoo-server/build.rs`.\n");
    code.push_str("// DO NOT EDIT MANUALLY.\n");
    code.push_str("// Source: `#[rpc_method]` macro on `cuckoo-service` methods.\n");
    code.push_str("//\n");
    code.push_str("// Generated fetch wrappers for all REST endpoints.\n");
    code.push_str("// Each function uses `apiFetch` from `../client.ts` and provides\n");
    code.push_str("// full TypeScript type safety for parameters and return values.\n\n");

    code.push_str("import { apiFetch } from \"../client\";\n\n");

    // ── 收集所有导出的类型（动态扫描 ts-rs 生成的文件）──
    let exported_types = scan_exported_types(output_dir);

    let all_types: Vec<String> = exported_types.clone();
    let mut used_types: Vec<String> = Vec::new();

    for route in routes {
        if let Some(bt) = &route.body_type {
            if !used_types.contains(bt) {
                used_types.push(bt.clone());
            }
        }
        // 从 return_type 提取类型名（去掉 [] 后缀）
        let ret_type = route.return_type.trim_end_matches("[]");
        if ret_type != "void" && !used_types.iter().any(|t| t == ret_type) {
            used_types.push(ret_type.to_string());
        }
    }

    // Re-export all types from types.ts
    code.push_str("// Re-export all types from types.ts for convenience\n");
    code.push_str("export type {\n");
    for ty in &all_types {
        code.push_str(&format!("  {},\n", ty));
    }
    code.push_str("} from \"./types\";\n\n");

    // Import only types used in function signatures
    code.push_str("// Import only types used in function signatures\n");
    code.push_str("import type {\n");
    for ty in &used_types {
        code.push_str(&format!("  {},\n", ty));
    }
    code.push_str("} from \"./types\";\n\n");

    // ── 为每个路由生成 fetch 函数 ──
    for route in routes {
        let ts_fn_name = &route.ts_fn_name;
        let method = &route.method;
        let path = &route.path;

        // 从 path 中提取描述
        let description = format!("{} {} {}", method, path, route.fn_name);

        code.push_str("/**\n");
        code.push_str(&format!(" * {}\n", description));
        code.push_str(&format!(" * {} {}\n", method, path));
        code.push_str(" */\n");

        // 构建函数参数
        let mut params: Vec<String> = Vec::new();
        for p in &route.path_params {
            params.push(format!("{}: string", p));
        }
        if let Some(body_type) = &route.body_type {
            params.push(format!("body: {}", body_type));
        }
        let params_str = params.join(", ");

        // 构建路径（替换 {param} 为 ${param}）
        let mut path_template = path.to_string();
        let has_params = !route.path_params.is_empty();
        for p in &route.path_params {
            path_template = path_template.replace(
                &format!("{{{}}}", p),
                &format!("${{encodeURIComponent({})}}", p),
            );
        }

        // 含参数的路径用反引号包裹（模板字符串），无参数的用双引号
        let path_literal = if has_params {
            format!("`{}`", path_template)
        } else {
            format!("\"{}\"", path_template)
        };

        // 构建请求 init
        let init = if route.body_type.is_some() {
            format!("{{ method: \"{}\", body: JSON.stringify(body) }}", method)
        } else {
            format!("{{ method: \"{}\" }}", method)
        };

        // 返回类型处理
        let return_expr = if route.return_type == "void" {
            format!("apiFetch<void>({}, {})", path_literal, init)
        } else {
            format!("apiFetch<{}>({}, {})", route.return_type, path_literal, init)
        };

        code.push_str(&format!(
            "export function {}({}): Promise<{}> {{\n  return {};\n}}\n\n",
            ts_fn_name,
            if params_str.is_empty() { "" } else { &params_str },
            route.return_type,
            return_expr,
        ));
    }

    let output_file = output_dir.join("api.ts");
    fs::write(&output_file, code).expect("failed to write generated api.ts");
}

/// 生成 `types.ts`：合并所有 ts-rs 生成的类型到单一文件。
///
/// ts-rs 生成的各个 .ts 文件之间缺少 import 语句（已知限制），
/// 将所有类型定义合并到 types.ts 中可消除跨文件引用问题。
/// 类型清单通过动态扫描目录获得，新增 `#[ts(export)]` 类型无需修改本文件。
fn generate_types_ts(output_dir: &Path) {
    let types_file = output_dir.join("types.ts");
    let mut types_code = String::new();
    types_code.push_str("// This file is auto-generated by `cuckoo-server/build.rs`.\n");
    types_code.push_str("// DO NOT EDIT MANUALLY.\n");
    types_code.push_str("// Merged from all ts-rs generated type files to avoid cross-file import issues.\n\n");

    let type_names = scan_exported_types(output_dir);

    for ty in &type_names {
        let file_path = output_dir.join(format!("{}.ts", ty));
        if let Ok(content) = fs::read_to_string(&file_path) {
            // 去掉 ts-rs 的自动生成注释和 import 语句，只保留类型定义
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("// This file was generated by")
                    || trimmed.starts_with("import ")
                {
                    continue;
                }
                types_code.push_str(line);
                types_code.push('\n');
            }
            types_code.push('\n');
        }
    }

    if type_names.is_empty() {
        types_code.push_str("// NOTE: no ts-rs generated type files found yet.\n");
        types_code.push_str("// Run `cargo test --workspace` to generate them, then rebuild cuckoo-server.\n\n");
    }

    fs::write(&types_file, types_code).expect("failed to write generated types.ts");

    // 注意：不删除 ts-rs 生成的单独类型文件。
    // 它们是 cargo test 的产物，保留在目录中供下次构建时扫描合并；
    // index.ts 只从 api.ts/types.ts 导出，单独文件不会产生重复导出。
}

/// 生成 `index.ts` barrel export。
fn generate_index_ts(output_dir: &Path) {
    let index_file = output_dir.join("index.ts");
    let index_code = "// This file is auto-generated by `cuckoo-server/build.rs`.\n// DO NOT EDIT MANUALLY.\n// Barrel export for all generated API code.\n\nexport * from \"./api\";\nexport * from \"./types\";\n";
    fs::write(&index_file, index_code).expect("failed to write generated index.ts");
}

/// 生成 CLI 用的 Rust 调用模块 `cli_generated.rs`。
///
/// 与 `api.ts` 平行，消费同一份 `.rpc_routes.json` 元数据。
/// 生成的内容写入 `apps/cuckoo-cli/src/generated/cli_generated.rs`，
/// 由 `cuckoo-cli` 通过 `mod generated;` 引入。
///
/// CLI 侧统一用 `serde_json::Value` 做 body 和返回值，不强绑定 Rust DTO 类型，
/// 因为 CLI 不依赖 `cuckoo-service`/`cuckoo-dto`（spec.md 2.2 节）。
fn generate_cli_rs(output_dir: &Path, routes: &[RpcRoute]) {
    let mut code = String::new();

    code.push_str("// This file is auto-generated by `cuckoo-server/build.rs`.\n");
    code.push_str("// DO NOT EDIT MANUALLY.\n");
    code.push_str("// Source: `#[rpc_method]` macro on `cuckoo-service` methods.\n");
    code.push_str("// CLI client wrappers for all REST endpoints.\n");
    code.push_str("// Each function takes (base_url, token, client, ...) and returns anyhow::Result<serde_json::Value>.\n\n");

    for route in routes {
        let fn_name = &route.fn_name;
        let method = &route.method;
        let path = &route.path;

        // 文档注释
        code.push_str(&format!("/// `{} {}` (auto-generated from #[rpc_method])\n", method, path));

        // 构建函数参数列表
        let mut params: Vec<String> = Vec::new();
        params.push("base_url: &str".to_string());
        params.push("token: &str".to_string());
        params.push("client: &reqwest::Client".to_string());
        for p in &route.path_params {
            params.push(format!("{}: &str", p));
        }
        if route.body_type.is_some() {
            params.push("body: &serde_json::Value".to_string());
        }
        let params_str = params.join(", ");

        // 构建 URL：将 {param} 替换为 format! 占位符
        let url_expr = if route.path_params.is_empty() {
            format!("format!(\"{{}}{}\", base_url)", path)
        } else {
            // 将 {param_name} 替换为 {}，然后用 format! 填充
            let mut fmt_path = path.to_string();
            for p in &route.path_params {
                fmt_path = fmt_path.replace(&format!("{{{}}}", p), "{}");
            }
            // 构建 format!("{}{}", base_url, format!("/api/...", id))
            let inner_args = route.path_params.join(", ");
            format!("format!(\"{{}}{{}}\", base_url, format!(\"{}\", {}))", fmt_path, inner_args)
        };

        // 构建请求
        let req_builder = if route.body_type.is_some() {
            format!(
                "let res = client.request(reqwest::Method::from_bytes(\"{}\".as_bytes()).unwrap(), url)\n        .bearer_auth(token)\n        .json(body)\n        .send().await?;",
                method
            )
        } else {
            format!(
                "let res = client.request(reqwest::Method::from_bytes(\"{}\".as_bytes()).unwrap(), url)\n        .bearer_auth(token)\n        .send().await?;",
                method
            )
        };

        code.push_str("pub async fn ");
        code.push_str(fn_name);
        code.push('(');
        code.push_str(&params_str);
        code.push_str(") -> anyhow::Result<serde_json::Value> {\n    let url = ");
        code.push_str(&url_expr);
        code.push_str(";\n    ");
        code.push_str(&req_builder);
        code.push_str("\n    let status = res.status();\n    let json: serde_json::Value = res.json().await?;\n    if !status.is_success() {\n        anyhow::bail!(\"HTTP {} {} failed: {} {}\", \"");
        code.push_str(method);
        code.push_str("\", \"");
        code.push_str(path);
        code.push_str("\", status, json);\n    }\n    Ok(json)\n}\n\n");
    }

    let output_file = output_dir.join("cli_generated.rs");
    fs::write(&output_file, code).expect("failed to write cli_generated.rs");
}
