//! TCP accept 循环 + 连接分流（`spec.md` 4.3 节，`plan.md` M2.2 节）。
//!
//! 每个 TCP 连接 spawn 一个 tokio task，按以下流程处理：
//! 1. peek 前几个字节判断是明文 HTTP 请求
//! 2. 解析第一行：CONNECT → 隧道处理；其他 → 显式代理转发
//! 3. CONNECT 隧道内递归走 sniff → TLS / HTTP 分支

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};

use crate::connect;
use crate::error::{ProxyError, ProxyResult};
use crate::forward;
use crate::handler::{
    FlowContext, HttpMessage, RequestAction, ResponseAction, SharedHandler,
};
use crate::http1;

/// 代理服务器句柄：用于停机。
pub struct ProxyServer {
    /// accept 循环的 JoinHandle。
    pub join_handle: tokio::task::JoinHandle<()>,
    /// 监听地址。
    pub listen_addr: SocketAddr,
}

/// 启动代理监听。
///
/// `port` 为 0 时由操作系统分配空闲端口。
/// 返回 `ProxyServer` 供 Service 层管理生命周期。
pub async fn start_proxy(
    port: u16,
    handler: SharedHandler,
    ca: Arc<cuckoo_ca::CaAuthority>,
) -> ProxyResult<ProxyServer> {
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .map_err(ProxyError::Io)?;
    let listen_addr = listener.local_addr()?;

    tracing::info!(%listen_addr, "proxy listener started");

    let join_handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let handler = handler.clone();
                    let ca = ca.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_connection(stream, peer_addr, handler, ca).await
                        {
                            tracing::warn!(?e, %peer_addr, "connection handling failed");
                        }
                    });
                }
                Err(e) => {
                    tracing::error!(?e, "accept failed, listener exiting");
                    break;
                }
            }
        }
    });

    Ok(ProxyServer {
        join_handle,
        listen_addr,
    })
}

/// 处理单个连接：peek 第一行判断 CONNECT 或普通 HTTP。
async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    handler: SharedHandler,
    ca: Arc<cuckoo_ca::CaAuthority>,
) -> ProxyResult<()> {
    tracing::debug!(%peer_addr, "new connection");

    // 读取第一行（peek 方式，不消费数据）
    let first_line = peek_first_line(&mut stream).await?;

    if first_line.starts_with("CONNECT ") {
        // CONNECT 隧道
        connect::handle_connect_tunnel(stream, &first_line, handler, ca).await
    } else {
        // 非 CONNECT：显式代理明文 HTTP 请求
        let (host, port) = parse_host_from_request(&first_line).unwrap_or(("localhost".to_string(), 80));
        let ctx = FlowContext::new(&host, port);
        handle_plain_http(stream, &host, port, handler, ctx).await
    }
}

/// Peek 第一行（以 \r\n 结束）。
///
/// 使用 `TcpStream::peek` 不消费数据，后续 handle_connect_tunnel 会重新读取。
async fn peek_first_line(stream: &mut TcpStream) -> ProxyResult<String> {
    let mut buf = [0u8; 8192];
    let n = stream.peek(&mut buf).await?;
    if n == 0 {
        return Err(ProxyError::ConnectionClosed);
    }

    let data = &buf[..n];
    // 找到第一个 \r\n
    if let Some(pos) = data.windows(2).position(|w| w == b"\r\n") {
        let line = String::from_utf8_lossy(&data[..pos]).to_string();
        Ok(line)
    } else {
        // 没找到 \r\n，返回所有数据作为第一行
        Ok(String::from_utf8_lossy(data).to_string())
    }
}

/// 从请求行解析目标 host 和 port。
///
/// 显式代理请求行格式：`GET http://host:port/path HTTP/1.1`
/// 非代理格式（直接请求）：`GET /path HTTP/1.1`，需要从 Host header 获取。
fn parse_host_from_request(request_line: &str) -> Option<(String, u16)> {
    // GET http://host:port/path HTTP/1.1
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let uri = parts[1];
    if uri.starts_with("http://") || uri.starts_with("https://") {
        // 绝对 URI
        let scheme_end = uri.find("://").unwrap() + 3;
        let rest = &uri[scheme_end..];
        let host_port = rest.split('/').next().unwrap_or(rest);

        if let Some(colon_pos) = host_port.rfind(':') {
            let host = &host_port[..colon_pos];
            let port: u16 = host_port[colon_pos + 1..].parse().unwrap_or(80);
            Some((host.to_string(), port))
        } else {
            // 无端口，默认 80
            Some((host_port.to_string(), 80))
        }
    } else {
        // 相对 URI，需要从 Host header 解析（M2 简化：返回默认）
        None
    }
}

/// 从（可能是绝对形式的）URI 解析上游目标。
///
/// MapRemote 等规则改写后 URI 指向新上游；返回 `None` 表示 origin-form
/// （无 scheme），此时沿用 ctx 的 CONNECT 目标。
fn upstream_from_uri(uri: &str) -> Option<(String, u16, bool)> {
    let scheme = if uri.starts_with("https://") {
        "https"
    } else if uri.starts_with("http://") {
        "http"
    } else {
        return None;
    };

    let rest = &uri[scheme.len() + 3..];
    let host_port = rest.split(['/', '?']).next()?;
    if host_port.is_empty() {
        return None;
    }
    let default_port: u16 = if scheme == "https" { 443 } else { 80 };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().unwrap_or(default_port)),
        None => (host_port, default_port),
    };
    Some((host.to_string(), port, scheme == "https"))
}

/// 解析请求的实际目标：绝对 URI 优先，否则从 Host header 修正。
///
/// 显式代理模式下 origin-form 请求（`GET /path`）没有可解析的目标，
/// 调用方只能给默认值（localhost:80）；Host header 才是权威目标。
/// 同时修正 ctx，保证 Flow 记录的 server_addr 正确。
fn resolve_target_from_req(
    req: &HttpMessage,
    fallback_host: &str,
    fallback_port: u16,
    ctx: &mut FlowContext,
) -> (String, u16) {
    let resolved = if req.uri.starts_with("http://") || req.uri.starts_with("https://") {
        upstream_from_uri(&req.uri).map(|(h, p, _)| (h, p))
    } else {
        req.header("host").and_then(|host| match host.rsplit_once(':') {
            Some((h, p)) => p.parse::<u16>().ok().map(|port| (h.to_string(), port)),
            None => Some((host.to_string(), 80)),
        })
    };

    match resolved {
        Some((h, p)) => {
            ctx.target_host = h.clone();
            ctx.target_port = p;
            (h, p)
        }
        None => (fallback_host.to_string(), fallback_port),
    }
}

/// 同步 Host header 与实际上游一致（MapRemote 改写上游后必须同步，
/// 否则虚拟主机路由/校验会失败）。默认端口不携带端口号。
fn sync_host_header(req: &mut HttpMessage, host: &str, port: u16) {
    let value = if port == 80 || port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let mut replaced = false;
    for (k, v) in req.headers.iter_mut() {
        if k.eq_ignore_ascii_case("host") {
            *v = value.clone();
            replaced = true;
        }
    }
    if !replaced {
        req.headers.push(("Host".to_string(), value));
    }
}

/// 解析转发目标：优先从改写后的绝对 URI（MapRemote），否则用 ctx 目标。
fn resolve_upstream(req: &HttpMessage, ctx_host: &str, ctx_port: u16) -> (String, u16, bool) {
    match upstream_from_uri(&req.uri) {
        Some((host, port, is_tls)) => (host, port, is_tls),
        None => (ctx_host.to_string(), ctx_port, ctx_port == 443),
    }
}

/// 处理明文 HTTP 请求（非 CONNECT，显式代理模式）。
///
/// 在 TcpStream 上操作：需要先消费 peek 的数据。
pub async fn handle_plain_http(
    mut stream: TcpStream,
    host: &str,
    port: u16,
    handler: SharedHandler,
    mut ctx: FlowContext,
) -> ProxyResult<()> {
    // 读取完整 HTTP 请求（peek 的数据会被重新读取）
    let req = http1::read_request(&mut stream).await?;

    // origin-form 请求（相对 URI）：调用方传入的默认 host（localhost:80）
    // 不可靠，从 Host header 修正目标地址（同步修正 ctx 供 Flow 展示）
    let (host, port) = resolve_target_from_req(&req, host, port, &mut ctx);

    // 调用 handler（M3: async trait，on_request 会回填 ctx.flow_id）
    let action = handler.on_request(&mut ctx, &req).await?;
    match action {
        RequestAction::Forward(mut req) => {
            // MapRemote 等规则可能改写了 URI 指向新上游：重新解析转发目标
            let (upstream_host, upstream_port, is_tls) = resolve_upstream(&req, &host, port);
            if upstream_from_uri(&req.uri).is_some() {
                sync_host_header(&mut req, &upstream_host, upstream_port);
            }
            let response = forward::forward_request(&upstream_host, upstream_port, is_tls, &req).await?;

            // 调用 handler 处理响应
            let resp_action = handler.on_response(&ctx, &response).await?;
            let final_response = match resp_action {
                ResponseAction::Forward(res) => res,
                ResponseAction::Pause(_, _) => {
                    response
                }
            };

            // 写回客户端
            http1::write_response(&mut stream, &final_response).await?;
        }
        RequestAction::Respond(resp) => {
            // 短路：直接返回给客户端
            http1::write_response(&mut stream, &resp).await?;
        }
        RequestAction::Pause(_, _) => {
            // M3: 请求阶段断点已在 handler 内部处理完，不应返回 Pause
            // 如果到达这里，说明 handler 实现有误，记录警告并关闭连接
            tracing::warn!("handler returned Pause, but breakpoint should be resolved internally");
        }
    }

    Ok(())
}

/// 在 TLS 流上处理明文 HTTP 请求。
///
/// TLS 握手已完成，内层就是明文 HTTP。
pub async fn handle_plain_http_on_tls<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    host: &str,
    port: u16,
    handler: SharedHandler,
    ctx: FlowContext,
) -> ProxyResult<()> {
    // 读取 HTTP 请求
    let req = http1::read_request(stream).await?;

    // 如果是绝对 URI，提取 path
    // 对于 TLS 终止后的请求，URI 通常是相对路径（如 /path），需要构造完整请求给上游
    let upstream_req = if req.uri.starts_with("http://") || req.uri.starts_with("https://") {
        req.clone()
    } else {
        // 相对 URI：构造绝对路径
        let scheme = if port == 443 { "https" } else { "http" };
        HttpMessage {
            uri: format!("{scheme}://{host}:{port}{}", req.uri),
            ..req.clone()
        }
    };

    // ctx 需要可变：on_request 会回填 flow_id 供 on_response 精确关联
    let mut ctx = ctx;

    // 调用 handler（M3: async trait）
    let action = handler.on_request(&mut ctx, &upstream_req).await?;
    match action {
        RequestAction::Forward(mut req) => {
            // 转发到上游；MapRemote 等规则可能改写了 URI 指向新上游，
            // 需重新解析目标并同步 Host header（HTTPS 上游需要 TLS）
            let (upstream_host, upstream_port, is_tls) = resolve_upstream(&req, host, port);
            if upstream_from_uri(&req.uri).is_some() {
                sync_host_header(&mut req, &upstream_host, upstream_port);
            }
            let response = forward::forward_request(&upstream_host, upstream_port, is_tls, &req).await?;

            // handler 处理响应
            let resp_action = handler.on_response(&ctx, &response).await?;
            let final_response = match resp_action {
                ResponseAction::Forward(res) => res,
                ResponseAction::Pause(_, _) => {
                    response
                }
            };

            // 写回客户端
            http1::write_response(stream, &final_response).await?;
        }
        RequestAction::Respond(resp) => {
            http1::write_response(stream, &resp).await?;
        }
        RequestAction::Pause(_, _) => {
            // M3: 请求阶段断点已在 handler 内部处理完
            tracing::warn!("handler returned Pause, but breakpoint should be resolved internally");
        }
    }

    Ok(())
}
