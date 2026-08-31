//! `cuckoo-desktop` —— Tauri 应用外壳，纯粹的"壳"，不含任何业务 command
//! （`spec.md` 2.1 节）。
//!
//! 启动时 `tokio::spawn` 拉起本地 `cuckoo-server`（同进程）专门服务业务 API；
//! `WebviewWindow` 仍然通过 Tauri 原生的 `tauri://` 自定义协议加载打包好的
//! 前端静态资源（不走 `http://127.0.0.1` 去拿页面，理由见 `spec.md` 2.2 节）。
//!
//! 退出钩子：应用退出（`RunEvent::Exit`）前执行 `server.shutdown()`，
//! 停止代理监听、取消挂起断点并恢复系统代理设置——否则用户开着代理
//! 退出应用后，系统代理残留指向已死端口，整机 HTTP 流量全部黑洞。

mod state;
mod system_commands;

use state::ServerState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cuckoo_desktop=info,cuckoo_server=info".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 在 Tauri 的异步运行时里同步等待 cuckoo-server 启动完成，
            // 拿到监听地址与 token 后放进 Tauri State，供 `get_server_token`
            // command 读取（spec.md 2.2 节第 3/5 点）。
            let handle = tauri::async_runtime::block_on(cuckoo_server::spawn_server(None))
                .expect("cuckoo-server 启动失败");

            tracing::info!(addr = %handle.addr, "cuckoo-server embedded and ready");

            app.manage(ServerState {
                addr: handle.addr,
                token: handle.token.clone(),
                // 保留句柄供退出钩子清理（join_handle 随进程退出一起结束，
                // 不需要主动 abort；drop 只会 detach task，继续在后台运行）
                shutdown_handle: std::sync::Mutex::new(Some(handle)),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![system_commands::get_server_token])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            // 退出前清理：停代理监听、取消挂起断点、恢复系统代理设置。
            // cleanup 失败也继续退出（尽力而为，不阻塞退出流程）。
            let handle = app_handle
                .try_state::<ServerState>()
                .and_then(|state| state.shutdown_handle.lock().ok().and_then(|mut g| g.take()));
            if let Some(handle) = handle {
                tracing::info!("app exiting, running server shutdown cleanup");
                tauri::async_runtime::block_on(handle.shutdown());
            }
        }
    });
}
