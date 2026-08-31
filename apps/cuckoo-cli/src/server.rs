//! Server 连接管理：token 读取、端口探测、headless Server 自动拉起。
//!
//! `cuckoo-server` 启动时在应用数据目录写入 `server.token` 文件（spec.md 7.5 节），
//! CLI 启动时读取该文件获取鉴权 token。Server 的端口信息写入 `server.port` 文件，
//! CLI 通过读取该文件找到运行中的 Server 地址。
//!
//! 如果检测不到运行中的 Server，CLI 可以：
//! - 对一次性命令：自动 fork `cuckoo-server --headless` 子进程，完成后杀掉。
//! - 对持续订阅命令：提示用户先执行 `cuckoo server start`。

use std::path::PathBuf;
use std::time::Duration;

/// 应用数据目录（如 `~/Library/Application Support/Cuckoo/`）。
fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Cuckoo")
}

/// `server.token` 文件路径——与 `cuckoo_server::auth::token_file_path()` 保持一致。
pub fn token_file_path() -> PathBuf {
    app_data_dir().join("server.token")
}

/// `server.port` 文件路径——Server 启动时写入监听端口，CLI 读取以连接。
pub fn port_file_path() -> PathBuf {
    app_data_dir().join("server.port")
}

/// 读取 token 文件内容（trim 后返回）。
pub fn read_token() -> anyhow::Result<String> {
    let path = token_file_path();
    let content = std::fs::read_to_string(&path)?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        anyhow::bail!("token file exists but is empty: {:?}", path);
    }
    Ok(trimmed.to_string())
}

/// 读取端口文件并拼装 base_url。
///
/// 返回 `http://127.0.0.1:<port>` 格式的 URL。
pub fn read_base_url() -> anyhow::Result<String> {
    let port_str = std::fs::read_to_string(port_file_path())?;
    let port: u16 = port_str.trim().parse()
        .map_err(|e| anyhow::anyhow!("invalid port in {:?}: {}", port_file_path(), e))?;
    Ok(format!("http://127.0.0.1:{}", port))
}

/// 探测本地 Server 是否在运行。
///
/// 通过向 `/healthz` 发送 GET 请求来判断。超时时间 2 秒。
pub async fn detect_server() -> Option<(String, String)> {
    let base_url = read_base_url().ok()?;
    let token = read_token().ok()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;

    let resp = client
        .get(format!("{}/healthz", base_url))
        .bearer_auth(&token)
        .send()
        .await
        .ok()?;

    if resp.status().is_success() {
        Some((base_url, token))
    } else {
        None
    }
}

/// 获取 Server 连接信息，如果未运行则返回 `None`。
///
/// 调用方可以根据返回值决定是自动拉起 Server 还是提示用户。
pub async fn connect_or_none() -> Option<(String, String, reqwest::Client)> {
    let (base_url, token) = detect_server().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .ok()?;
    Some((base_url, token, client))
}

/// 确保 Server 在运行：如果未检测到，拉起 headless 子进程。
///
/// 返回 `(base_url, token, client)`。
/// `server_bin` 是 `cuckoo-server` 二进制路径，通常通过 `std::env::current_exe()`
/// 推导同级二进制。
pub async fn ensure_server(server_bin: Option<&str>) -> anyhow::Result<(String, String, reqwest::Client)> {
    // 先尝试连接已运行的 Server
    if let Some((base_url, token, client)) = connect_or_none().await {
        return Ok((base_url, token, client));
    }

    // 拉起 headless Server 子进程
    let bin = match server_bin {
        Some(p) => p.to_string(),
        None => find_server_binary()?,
    };

    eprintln!("[cuckoo] starting headless cuckoo-server: {}", bin);
    let mut child = std::process::Command::new(&bin)
        .arg("--headless")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start cuckoo-server: {} (path: {})", e, bin))?;

    // 等待 Server 就绪（轮询 /healthz，最多 10 秒）
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Some((base_url, token)) = detect_server().await {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?;
            return Ok((base_url, token, client));
        }
    }

    // 超时，杀掉子进程
    let _ = child.kill();
    anyhow::bail!("cuckoo-server failed to start within 10 seconds")
}

/// 尝试推导 `cuckoo-server` 二进制路径。
///
/// 策略：取当前可执行文件所在目录，查找 `cuckoo-server` 同级文件。
fn find_server_binary() -> anyhow::Result<String> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| anyhow::anyhow!("cannot determine exe parent dir"))?;

    let candidate = dir.join("cuckoo-server");
    if candidate.exists() {
        return Ok(candidate.to_string_lossy().to_string());
    }

    // 开发环境下可能需要从 target/debug 或 target/release 查找
    // 但这需要知道 workspace root，暂时 fallback 到 PATH 查找
    Ok("cuckoo-server".to_string())
}
