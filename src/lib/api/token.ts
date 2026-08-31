/**
 * 前端鉴权 token 管理（`spec.md` 6.3 节、2.2 节第 5 点）。
 *
 * 页面经 `tauri://` 加载后，前端主动调用极薄的 `get_server_token()` Tauri
 * command 拉取 token（比"URL 参数/全局变量注入"更干净，token 不会残留在
 * URL 或页面 HTML 里）。拿到后存在内存里，后续 `fetch` 请求携带在
 * `Authorization` 头里；`EventSource` 请求拼在 `?token=` query 参数里
 * （浏览器 `EventSource` 原生不支持自定义请求头，见 `spec.md` 7.5 节）。
 *
 * 浏览器开发模式 fallback：当 `invoke` 不可用时（非 Tauri 环境），
 * 从 `localStorage` 或环境变量中读取 token 和 base URL。
 */

export interface ServerTokenResponse {
  base_url: string;
  token: string;
}

let cached: ServerTokenResponse | null = null;

/** 启动时调用一次，拿到 token 与 `cuckoo-server` 的 base URL。 */
export async function getServerToken(): Promise<ServerTokenResponse> {
  if (cached) return cached;

  // 尝试 Tauri 环境
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    cached = await invoke<ServerTokenResponse>("get_server_token");
    return cached;
  } catch {
    // Tauri 不可用（浏览器开发模式），使用 fallback
  }

  // 浏览器开发模式 fallback：
  // 从 localStorage 读取，或使用 Vite 环境变量
  const baseUrl =
    localStorage.getItem("cuckoo_server_base_url") ||
    import.meta.env.VITE_CUCKOO_SERVER_URL ||
    "http://127.0.0.1:53935";

  const token =
    localStorage.getItem("cuckoo_server_token") ||
    import.meta.env.VITE_CUCKOO_SERVER_TOKEN ||
    "";

  if (!token) {
    throw new Error(
      "Server token not available. In browser dev mode, set localStorage 'cuckoo_server_token' and 'cuckoo_server_base_url', or use Tauri runtime.",
    );
  }

  cached = { base_url: baseUrl, token };
  return cached;
}
