//! 自研 HTTP/1.1 报文状态机（`spec.md` 4.2 节，`plan.md` M2.2 节）。
//!
//! M2 最小版本：
//! - 解析 request-line + headers（保留原始顺序与大小写）
//! - 支持 Content-Length body 解析
//! - 支持 chunked transfer-encoding（基本分块解析）
//! - 不支持 keep-alive 连接复用（每个请求一个连接，M5 补齐）
//! - 不支持 HTTP/1.0 的各种边界情况
//!
//! 写回时的 framing 规范化（关键正确性保证）：
//! - 解析阶段 chunked body 已被解块为完整 `Vec<u8>`，因此序列化时
//!   **必须丢弃原始 `Transfer-Encoding` header**，否则会出现
//!   `Transfer-Encoding: chunked` + 未分块裸 body 的协议违规组合。
//! - 原始 `Content-Length` 一律丢弃，按实际 body 长度唯一写入，
//!   避免 Rewrite/断点编辑改写 body 后长度不一致（请求走私风险）。
//! - `Proxy-Connection` 是 hop-by-hop header，不向任一方向转发。

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{ProxyError, ProxyResult};
use crate::handler::HttpMessage;

/// header 区（起始行 + 所有 header）大小上限，超出视为恶意请求。
pub const MAX_HEADER_BYTES: usize = 64 * 1024;
/// body 总大小上限（Content-Length 与 chunked 累计共用）。
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
/// 单个 chunk 大小上限。
const MAX_CHUNK_BYTES: usize = 16 * 1024 * 1024;

/// 判断消息是否按 chunked 编码传输。
///
/// 真实世界里常见 `Transfer-Encoding: gzip, chunked`（多个编码叠加），
/// 只要编码列表以 `chunked` 结尾，最外层就是 chunked 分块。
fn is_chunked(headers: &[(String, String)]) -> bool {
    headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("transfer-encoding")
            && v.to_ascii_lowercase().ends_with("chunked")
    })
}

/// 从流中读取一个完整的 HTTP/1.1 请求。
///
/// 解析流程：request-line → headers（\r\n\r\n 结束）→ body（Content-Length 或 chunked）。
pub async fn read_request<R: AsyncRead + Unpin>(reader: &mut R) -> ProxyResult<HttpMessage> {
    // 1. 读取 request-line + headers（直到 \r\n\r\n）
    let header_data = read_until_header_end(reader).await?;

    // 解析 header 文本
    let header_text = String::from_utf8_lossy(&header_data);
    let mut lines = header_text.lines();

    // request-line: METHOD SP URI SP VERSION
    let request_line = lines.next().ok_or_else(|| {
        ProxyError::HttpParse("empty request: no request-line".to_string())
    })?;

    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() != 3 {
        return Err(ProxyError::HttpParse(format!(
            "malformed request-line: {request_line}"
        )));
    }

    let method = parts[0].to_string();
    let uri = parts[1].to_string();
    let version = parts[2].to_string();

    // 解析 headers
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find(':') {
            let name = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            headers.push((name, value));
        }
    }

    // 2. 读取 body
    let body = read_body(reader, &headers).await?;

    Ok(HttpMessage {
        method,
        uri,
        version,
        headers,
        body,
    })
}

/// 从流中读取一个完整的 HTTP/1.1 响应。
///
/// `request_method` 是触发此响应的请求方法：HEAD 请求的响应
/// 按 RFC 9110 §9.3.2 不携带 body（即使声明了 Content-Length），
/// 忽略此参数会导致代理在上游空等 body 而挂起。
pub async fn read_response<R: AsyncRead + Unpin>(
    reader: &mut R,
    request_method: &str,
) -> ProxyResult<HttpMessage> {
    // 1. 读取 status-line + headers
    let header_data = read_until_header_end(reader).await?;
    let header_text = String::from_utf8_lossy(&header_data);
    let mut lines = header_text.lines();

    // status-line: HTTP/VERSION SP STATUS_CODE SP REASON
    let status_line = lines.next().ok_or_else(|| {
        ProxyError::HttpParse("empty response: no status-line".to_string())
    })?;

    let parts: Vec<&str> = status_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(ProxyError::HttpParse(format!(
            "malformed status-line: {status_line}"
        )));
    }

    let version = parts[0].to_string();
    let status_code = parts[1].to_string();
    let reason = parts.get(2).copied().unwrap_or("");

    // headers
    let mut headers = Vec::new();
    headers.push((":status".to_string(), format!("{status_code} {reason}")));

    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find(':') {
            let name = line[..pos].trim().to_string();
            let value = line[pos + 1..].trim().to_string();
            headers.push((name, value));
        }
    }

    // 2. 读取 body：
    // - HEAD 请求：响应永远没有 body
    // - 204/304/1xx：状态码本身禁止携带 body
    let has_no_body = request_method.eq_ignore_ascii_case("HEAD")
        || status_code == "204"
        || status_code == "304"
        || status_code.starts_with('1');
    let body = if has_no_body {
        Vec::new()
    } else {
        // 读取错误（连接中断、畸形分块等）必须向上传播：
        // 吞掉错误会导致半截 body 被当作完整响应回写，
        // 配合 Content-Length 令客户端永久挂起。
        read_body(reader, &headers).await?
    };

    Ok(HttpMessage {
        method: String::new(), // response 没有 method
        uri: String::new(),
        version,
        headers,
        body,
    })
}

/// 将 HTTP 响应写回客户端（HTTP/1.1 格式）。
///
/// 序列化前规范化 framing headers（见模块文档）。
pub async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &HttpMessage,
) -> ProxyResult<()> {
    let status = msg.header(":status").unwrap_or("200 OK");

    // 204/1xx 状态码禁止携带 Content-Length（RFC 9110 §8.6），
    // 304 语义上 body 长度无效，统一不写。
    let code = status.split_whitespace().next().unwrap_or("");
    let no_body_status = code == "204" || code == "304" || code.starts_with('1');

    // status-line
    writer
        .write_all(format!("HTTP/1.1 {status}\r\n").as_bytes())
        .await?;

    write_headers_and_body(writer, msg, no_body_status).await?;
    writer.flush().await?;
    Ok(())
}

/// 将 HTTP 请求写入上游连接（HTTP/1.1 格式）。
///
/// 供 `forward.rs` 使用，与 `write_response` 共享同一套
/// framing 规范化逻辑，保证转发与回写行为一致。
pub async fn write_request<W: AsyncWrite + Unpin>(
    writer: &mut W,
    req: &HttpMessage,
) -> ProxyResult<()> {
    // request-line
    writer
        .write_all(format!("{} {} {}\r\n", req.method, req.uri, req.version).as_bytes())
        .await?;

    write_headers_and_body(writer, req, false).await?;
    writer.flush().await?;
    Ok(())
}

/// 写入 headers + body，规范化消息帧（framing）相关的 header。
///
/// 规则（见模块文档）：
/// - 跳过 `:status` 伪 header（起始行由调用方写）
/// - 丢弃原始 `Content-Length` / `Transfer-Encoding`，随后按 body 实际长度唯一写入
/// - 跳过 hop-by-hop 的 `Proxy-Connection`
///
/// `omit_content_length` 为 true 时不写 `Content-Length`
/// （用于 204/304/1xx 响应），body 也不会写出（这些状态码没有 body）。
async fn write_headers_and_body<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &HttpMessage,
    omit_content_length: bool,
) -> ProxyResult<()> {
    for (name, value) in &msg.headers {
        let lower = name.to_ascii_lowercase();
        if lower == ":status"
            || lower == "content-length"
            || lower == "transfer-encoding"
            || lower == "proxy-connection"
        {
            continue;
        }
        writer
            .write_all(format!("{name}: {value}\r\n").as_bytes())
            .await?;
    }

    if omit_content_length {
        writer.write_all(b"\r\n").await?;
    } else {
        // 按实际 body 长度唯一写入 Content-Length（含空 body 的 0），
        // 使客户端无需依赖连接关闭即可确定消息边界。
        writer
            .write_all(format!("Content-Length: {}\r\n\r\n", msg.body.len()).as_bytes())
            .await?;
        writer.write_all(&msg.body).await?;
    }

    Ok(())
}

/// 读取直到 \r\n\r\n（header 结束标记）。
async fn read_until_header_end<R: AsyncRead + Unpin>(reader: &mut R) -> ProxyResult<Vec<u8>> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];

    loop {
        let n = reader.read(&mut byte).await.map_err(|e| {
            ProxyError::Io(io::Error::new(
                e.kind(),
                format!("read header byte: {e}"),
            ))
        })?;

        if n == 0 {
            return Err(ProxyError::ConnectionClosed);
        }

        buf.push(byte[0]);

        // 大小上限：防止畸形客户端发送永不结束的 header 导致 OOM
        if buf.len() > MAX_HEADER_BYTES {
            return Err(ProxyError::HttpParse(format!(
                "header section exceeds {MAX_HEADER_BYTES} bytes limit"
            )));
        }

        // 检测 \r\n\r\n
        if buf.len() >= 4 {
            let tail = &buf[buf.len() - 4..];
            if tail == b"\r\n\r\n" {
                // 去掉末尾的 \r\n\r\n
                buf.truncate(buf.len() - 4);
                return Ok(buf);
            }
        }
    }
}

/// 根据 headers 读取 body。
async fn read_body<R: AsyncRead + Unpin>(
    reader: &mut R,
    headers: &[(String, String)],
) -> ProxyResult<Vec<u8>> {
    // 检查 Transfer-Encoding: chunked（含 "gzip, chunked" 等叠加形式）
    if is_chunked(headers) {
        return read_chunked_body(reader).await;
    }

    // 检查 Content-Length
    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok());

    if let Some(len) = content_length {
        if len == 0 {
            return Ok(Vec::new());
        }
        if len > MAX_BODY_BYTES {
            return Err(ProxyError::HttpParse(format!(
                "content-length {len} exceeds {MAX_BODY_BYTES} bytes limit"
            )));
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        return Ok(buf);
    }

    // 无 Content-Length 且非 chunked：body 为空（GET 等无 body 请求）
    Ok(Vec::new())
}

/// 读取 chunked 编码的 body。
async fn read_chunked_body<R: AsyncRead + Unpin>(reader: &mut R) -> ProxyResult<Vec<u8>> {
    let mut body = Vec::new();
    let mut line_buf = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        line_buf.clear();
        // 读取一行（chunk size 行）
        loop {
            let n = reader.read(&mut byte).await?;
            if n == 0 {
                return Err(ProxyError::ConnectionClosed);
            }
            line_buf.push(byte[0]);
            if line_buf.len() > MAX_HEADER_BYTES {
                return Err(ProxyError::HttpParse(
                    "chunk size line exceeds limit".to_string(),
                ));
            }
            if line_buf.ends_with(b"\r\n") {
                break;
            }
        }

        // 解析 chunk size（十六进制，可能带 extensions）
        let size_line = String::from_utf8_lossy(&line_buf).to_string();
        let size_str = size_line
            .trim()
            .split(';')
            .next()
            .unwrap_or("0")
            .trim();
        let chunk_size = usize::from_str_radix(size_str, 16)
            .map_err(|e| ProxyError::HttpParse(format!("invalid chunk size '{size_str}': {e}")))?;

        if chunk_size == 0 {
            // 读取 trailer 部分（可能有若干 trailer header 行 + 空行），
            // EOF 时宽容退出（部分实现不带结尾空行）。
            loop {
                line_buf.clear();
                let mut eof = false;
                loop {
                    let n = reader.read(&mut byte).await?;
                    if n == 0 {
                        eof = true;
                        break;
                    }
                    line_buf.push(byte[0]);
                    if line_buf.ends_with(b"\r\n") {
                        break;
                    }
                }
                if eof || line_buf == b"\r\n" {
                    break;
                }
            }
            break;
        }

        // 单 chunk 与累计大小上限：防止 `FFFFFFFF` 之类的恶意 size 直接 OOM
        if chunk_size > MAX_CHUNK_BYTES || body.len() + chunk_size > MAX_BODY_BYTES {
            return Err(ProxyError::HttpParse(format!(
                "chunked body exceeds {MAX_BODY_BYTES} bytes limit"
            )));
        }

        // 读取 chunk data
        let mut chunk = vec![0u8; chunk_size];
        reader.read_exact(&mut chunk).await?;
        body.extend_from_slice(&chunk);

        // 读取 chunk 后的 \r\n
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf).await;
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn make_req(body: &[u8], extra_headers: &[(&str, &str)]) -> HttpMessage {
        let mut headers = vec![
            ("Host".to_string(), "example.com".to_string()),
            ("Content-Length".to_string(), body.len().to_string()),
        ];
        for (k, v) in extra_headers {
            headers.push((k.to_string(), v.to_string()));
        }
        HttpMessage {
            method: "POST".to_string(),
            uri: "/path".to_string(),
            version: "HTTP/1.1".to_string(),
            headers,
            body: body.to_vec(),
        }
    }

    /// 回写不得产生重复 Content-Length / 残留 Transfer-Encoding（C1 回归测试）。
    #[tokio::test]
    async fn test_write_request_no_duplicate_content_length() {
        let mut buf = Vec::new();
        let req = make_req(b"hello", &[("Transfer-Encoding", "chunked")]);
        write_request(&mut buf, &req).await.unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.matches("Content-Length:").count(), 1);
        assert!(!text.to_lowercase().contains("transfer-encoding"));
        assert!(text.contains("Content-Length: 5"));
    }

    /// 204 响应不得携带 Content-Length。
    #[tokio::test]
    async fn test_write_response_204_no_content_length() {
        let mut buf = Vec::new();
        let res = HttpMessage {
            method: String::new(),
            uri: String::new(),
            version: "HTTP/1.1".to_string(),
            headers: vec![(":status".to_string(), "204 No Content".to_string())],
            body: Vec::new(),
        };
        write_response(&mut buf, &res).await.unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(!text.contains("Content-Length"));
    }

    /// HEAD 响应不读 body（I8 回归测试）。
    #[tokio::test]
    async fn test_read_response_head_no_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n";
        let mut cursor = Cursor::new(raw.to_vec());
        let res = read_response(&mut cursor, "HEAD").await.unwrap();
        assert!(res.body.is_empty());
    }

    /// `Transfer-Encoding: gzip, chunked` 应按 chunked 解析（I14 回归测试）。
    #[tokio::test]
    async fn test_read_body_gzip_chunked() {
        // "hello" 一个 chunk + 结束 chunk
        let mut raw = b"POST / HTTP/1.1\r\nHost: a.com\r\nTransfer-Encoding: gzip, chunked\r\n\r\n".to_vec();
        raw.extend_from_slice(b"5\r\nhello\r\n0\r\n\r\n");
        let mut cursor = Cursor::new(raw);
        let req = read_request(&mut cursor).await.unwrap();
        assert_eq!(req.body, b"hello");
    }

    /// 超大 Content-Length 应被拒绝（C5 回归测试）。
    #[tokio::test]
    async fn test_reject_oversized_content_length() {
        let raw =
            b"POST / HTTP/1.1\r\nHost: a.com\r\nContent-Length: 999999999999\r\n\r\n";
        let mut cursor = Cursor::new(raw.to_vec());
        let result = read_request(&mut cursor).await;
        assert!(result.is_err());
    }

    /// 超大 chunk size 应被拒绝（C5 回归测试）。
    #[tokio::test]
    async fn test_reject_oversized_chunk() {
        let mut raw =
            b"POST / HTTP/1.1\r\nHost: a.com\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
        raw.extend_from_slice(b"FFFFFFFF\r\n");
        let mut cursor = Cursor::new(raw);
        let result = read_request(&mut cursor).await;
        assert!(result.is_err());
    }
}
