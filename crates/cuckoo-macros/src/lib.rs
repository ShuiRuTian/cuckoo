//! `cuckoo-macros`：提供 `#[rpc_method(METHOD, PATH)]` 属性宏（`spec.md` 2.3 节）。
//!
//! 宏在编译期做两件事：
//! 1. 解析函数签名，提取完整的路由元信息（method/path/fn_name/body_type/return_type/path_params），
//!    写入 `<crate>/.rpc_routes/<fn_name>.json`，供 `cuckoo-server/build.rs` 扫描合并
//!    生成前端 TS 客户端。每个方法一个独立文件，天然幂等且无并发竞态
//!    （cargo 并行编译 lib/test 多个编译单元时不会丢失更新）。
//! 2. 通过 `inventory::submit!` 把元信息登记进全局清单，供 server 启动期路由表打印/自检。
//!
//! 参数分类规则（基于函数签名的参数类型名）：
//! - `DatabaseConnection` 或以 `&` 开头的引用类型 → 注入参数，不暴露给前端
//! - `String` → 路径参数（与 path 中的 `{param}` 对应）
//! - 其他（通常是 `*Input` 类型）→ 请求 body
//!
//! 返回类型解析：
//! - `ServiceResult<T>` → 提取 T
//! - `ServiceResult<Vec<T>>` → 提取 T，标记为数组
//! - `ServiceResult<()>` → void
//! - 直接返回 `T`（非 ServiceResult） → 提取 T

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, ReturnType, Type};

/// `#[rpc_method("GET", "/api/ping")]`
///
/// 展开后：
/// 1. 保留原函数定义不变；
/// 2. 解析签名，将路由元信息写入 `OUT_DIR/rpc_routes.json`；
/// 3. 生成 `inventory::submit!`，把 `(method, path, fn_name)` 登记进全局清单。
#[proc_macro_attribute]
pub fn rpc_method(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(
        attr with syn::punctuated::Punctuated::<LitStr, syn::Token![,]>::parse_terminated
    );
    let mut iter = args.into_iter();
    let method = match iter.next() {
        Some(lit) => lit.value(),
        None => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "rpc_method 需要两个参数：#[rpc_method(\"GET\", \"/api/ping\")]",
            )
            .to_compile_error()
            .into();
        }
    };
    let path = match iter.next() {
        Some(lit) => lit.value(),
        None => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "rpc_method 需要两个参数：#[rpc_method(\"GET\", \"/api/ping\")]",
            )
            .to_compile_error()
            .into();
        }
    };

    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_ident = &input_fn.sig.ident;
    let fn_name = fn_ident.to_string();
    let register_ident = quote::format_ident!("__rpc_register_{}", fn_ident);

    // ── 解析函数参数 ──
    let mut path_params: Vec<String> = Vec::new();
    let mut body_type: Option<String> = None;

    for arg in &input_fn.sig.inputs {
        if let FnArg::Typed(pat_type) = arg {
            let type_str = type_to_string(&pat_type.ty);
            // 注入参数：DatabaseConnection 或引用类型（&RuleState/&ProxyState 等状态注入）
            if type_str.contains("DatabaseConnection") || type_str.starts_with('&') {
                continue;
            }
            // 路径参数：String 类型（与 path 中的 {param} 对应）
            if type_str == "String" {
                if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                    path_params.push(pat_ident.ident.to_string());
                }
                continue;
            }
            // 请求 body：仅 DTO 类型（大写开头的自定义类型，如 CreateRuleInput）。
            // 基础类型（u16/f64/usize 等）视为服务端注入参数，不暴露给前端。
            if type_str.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                body_type = Some(type_str);
            }
        }
    }

    // ── 解析返回类型 ──
    let return_type_str = extract_return_type(&input_fn.sig.output);

    // ── 提取路径中的参数名（用于交叉验证）──
    // axum 0.8 语法：`/api/rules/{id}`
    let path_param_names: Vec<String> = path
        .split('/')
        .filter_map(|segment| {
            segment
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .map(|s| s.to_string())
        })
        .collect();

    // ── 写入 JSON 清单（每方法一个独立文件，幂等且无并发竞态）──
    // proc_macro 中 OUT_DIR 不一定被设置（只有有 build.rs 的 crate 才有），
    // 但 CARGO_MANIFEST_DIR 始终可用。写入 manifest 目录下的 .rpc_routes/<fn_name>.json，
    // build.rs 扫描各 crate 的该目录来收集。
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let routes_dir = std::path::PathBuf::from(&manifest_dir).join(".rpc_routes");
        if std::fs::create_dir_all(&routes_dir).is_ok() {
            // 生成 camelCase 函数名
            let ts_fn_name = snake_to_camel(&fn_name);

            let entry = serde_json::json!({
                "method": method,
                "path": path,
                "fn_name": fn_name,
                "ts_fn_name": ts_fn_name,
                "body_type": body_type,
                "return_type": return_type_str,
                "path_params": path_params,
                "path_param_names": path_param_names,
            });

            let json_path = routes_dir.join(format!("{}.json", fn_name));
            if let Ok(json_str) = serde_json::to_string(&entry) {
                let _ = std::fs::write(&json_path, json_str);
            }
        }
    }

    let method_lit = method;
    let path_lit = path;
    let fn_name_lit = fn_name;

    let expanded = quote! {
        #input_fn

        #[allow(non_upper_case_globals)]
        ::cuckoo_core::rpc_registry::inventory::submit! {
            ::cuckoo_core::rpc_registry::RpcMethodDescriptor {
                method: #method_lit,
                path: #path_lit,
                fn_name: #fn_name_lit,
            }
        }

        // 保留一个防止 unused import 警告的锚点。
        #[allow(dead_code)]
        fn #register_ident() {}
    };

    expanded.into()
}

/// 将 `syn::Type` 转换为可读的字符串表示。
fn type_to_string(ty: &Type) -> String {
    quote::quote!(#ty).to_string().replace(' ', "")
}

/// 从返回类型中提取 TypeScript 类型名。
///
/// - `ServiceResult<T>` → "T"
/// - `ServiceResult<Vec<T>>` → "T[]"
/// - `ServiceResult<()>` → "void"
/// - 直接返回 `T`（非 ServiceResult） → "T"
fn extract_return_type(output: &ReturnType) -> String {
    let return_type = match output {
        ReturnType::Type(_, ty) => &**ty,
        ReturnType::Default => return "void".to_string(),
    };

    let type_str = type_to_string(return_type);

    // 尝试匹配 ServiceResult<...>
    if let Some(inner) = extract_generic_arg(&type_str, "ServiceResult") {
        // inner 可能是 "T", "Vec<T>", "()"
        if inner == "()" {
            return "void".to_string();
        }
        if let Some(vec_inner) = extract_generic_arg(&inner, "Vec") {
            return format!("{}[]", dto_to_ts_type(&vec_inner));
        }
        return dto_to_ts_type(&inner);
    }

    // 非 ServiceResult 的直接返回类型
    dto_to_ts_type(&type_str)
}

/// 从 `"GenericName<InnerType>"` 格式中提取 `InnerType`。
fn extract_generic_arg(type_str: &str, generic_name: &str) -> Option<String> {
    let prefix = format!("{}<", generic_name);
    if type_str.starts_with(&prefix) && type_str.ends_with('>') {
        Some(type_str[prefix.len()..type_str.len() - 1].to_string())
    } else {
        None
    }
}

/// 将 Rust DTO 类型名映射为 TypeScript 类型名。
///
/// 规则：
/// - `WorkspaceDto` → `WorkspaceModel`
/// - `FolderDto` → `FolderModel`
/// - `HttpRequestDefDto` → `HttpRequestDefModel`
/// - `EnvironmentDto` → `EnvironmentModel`
/// - `*Input` → 保持不变
/// - 其他 → 保持不变
fn dto_to_ts_type(rust_type: &str) -> String {
    match rust_type {
        "WorkspaceDto" => "WorkspaceModel".to_string(),
        "FolderDto" => "FolderModel".to_string(),
        "HttpRequestDefDto" => "HttpRequestDefModel".to_string(),
        "EnvironmentDto" => "EnvironmentModel".to_string(),
        other => other.to_string(),
    }
}

/// 将 snake_case 转换为 camelCase。
fn snake_to_camel(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}
