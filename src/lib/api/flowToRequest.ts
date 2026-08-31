/**
 * Flow → Collection 请求转换工具（`plan.md` M3.3 联动功能）。
 *
 * 把代理捕获的 `Flow.request`（origin-form URI + 原始 headers/body）
 * 转换成 `AdHocRequest`（重发用）或 `CreateRequestInput`（另存为用）。
 */

import type { AdHocRequest, CreateRequestInput, Flow } from "@/lib/api/generated";

/** 重发/另存时应跳过的 hop-by-hop 或由客户端重新计算的 header */
const SKIP_HEADERS = new Set([
  "content-length",
  "connection",
  "proxy-connection",
  "keep-alive",
  "transfer-encoding",
  "upgrade",
]);

/** 从 Flow 提取完整 URL（scheme + host + path + query） */
export function flowToUrl(flow: Flow): string {
  const req = flow.request;
  const host = req.headers.find(([k]) => k.toLowerCase() === "host")?.[1]
    ?? flow.server_addr?.ip
    ?? "";
  const port = flow.server_addr?.port;
  const defaultPort = flow.tls ? 443 : 80;

  // host 可能已带端口
  const hostWithPort =
    port && port !== defaultPort && !host.includes(":")
      ? `${host}:${port}`
      : host;

  const scheme = flow.tls ? "https" : "http";
  // URI 是 origin-form（/path?query）；若是 absolute-form 直接用
  const uri = req.uri.startsWith("http") ? req.uri : `//${hostWithPort}${req.uri}`;
  return uri.startsWith("http") ? uri : `${scheme}:${uri}`;
}

/** 从 Flow 提取 Content-Type */
function getContentType(flow: Flow): string {
  return (
    flow.request.headers.find(([k]) => k.toLowerCase() === "content-type")?.[1] ??
    "text/plain"
  );
}

/** Flow body → 文本 */
function bodyToText(flow: Flow): string {
  const body = flow.request.body;
  if (body.length === 0) return "";
  try {
    return new TextDecoder().decode(new Uint8Array(body));
  } catch {
    return "";
  }
}

/** Flow → ad-hoc 请求（重发用） */
export function flowToAdHoc(flow: Flow): AdHocRequest {
  const url = flowToUrl(flow);

  // query 保留在 URL 中不拆分，提高重放保真度
  return {
    method: flow.request.method,
    url,
    headers: flow.request.headers
      .filter(([k]) => !SKIP_HEADERS.has(k.toLowerCase()))
      .map(([name, value]) => ({ name, value, enabled: true })),
    query_params: [],
    body:
      flow.request.body.length > 0
        ? { type: "raw", content_type: getContentType(flow), text: bodyToText(flow) }
        : { type: "none" },
    auth: { type: "none" },
  };
}

/** Flow → 另存为 Collection 请求的输入 */
export function flowToCreateInput(
  flow: Flow,
  workspaceId: string,
  folderId: string | null,
): CreateRequestInput {
  const adHoc = flowToAdHoc(flow);
  // 名称默认取 path 最后一段
  let name = flow.request.uri.split("?")[0].split("/").filter(Boolean).pop() ?? "captured request";
  name = `${name} (captured)`;

  return {
    workspace_id: workspaceId,
    folder_id: folderId,
    name,
    method: adHoc.method,
    url: adHoc.url,
    headers: adHoc.headers,
    query_params: [],
    body: adHoc.body,
    auth: adHoc.auth,
  };
}
