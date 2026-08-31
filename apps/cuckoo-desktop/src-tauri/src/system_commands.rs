//! 极少数必须原生实现的能力（`spec.md` 2.1 节 `system_commands.rs` 职责说明）：
//! 托盘菜单、原生文件对话框、开机自启注册（均留待后续里程碑实现），以及
//! `get_server_token()`——**唯一一个业务相关的 Tauri command**，供前端启动时
//! 主动拉取鉴权 token（见 `spec.md` 6.3 节、2.2 节第 5 点）。
//!
//! 之所以只有这一个业务相关 command：桌面 UI 的业务 API 调用全部走
//! `fetch`/`EventSource` 访问 `cuckoo-server`（`http://127.0.0.1:<port>`），
//! 不会再新增第二个 Tauri command 作为业务逻辑的入口——那样会重新引入
//! "两套协议、两套胶水代码"的问题（`spec.md` 2.2 节）。

use tauri::State;

use crate::state::ServerState;

/// 前端启动时通过 `invoke('get_server_token')` 主动拉取鉴权 token 与
/// `cuckoo-server` 监听地址。
///
/// 因为页面是经 `tauri://` 加载的（不是普通网页），可以直接调用 Tauri 提供的
/// `invoke` API 拿到 token，比"URL 参数/页面注入全局变量"这类专为普通网页
/// 设计的变通方案更干净（token 不会残留在 URL 或页面 HTML 里）。
#[tauri::command]
pub fn get_server_token(state: State<'_, ServerState>) -> ServerTokenResponse {
    ServerTokenResponse {
        base_url: format!("http://{}", state.addr),
        token: state.token.clone(),
    }
}

#[derive(serde::Serialize)]
pub struct ServerTokenResponse {
    pub base_url: String,
    pub token: String,
}
