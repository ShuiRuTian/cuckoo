import { getServerToken } from "./token";

/**
 * 强类型 API 客户端封装（`spec.md` 2.3 节、6.3 节）。
 *
 * M0 阶段先手写一个最小封装；M1 起由 `build.rs` 从 `#[rpc_method]` 方法清单
 * 自动生成 `lib/api/generated.ts` 里的强类型 `fetch` 封装函数，本文件只保留
 * 通用的请求基础设施（base URL + token 注入 + 错误处理）。
 */

/** 所有 REST 请求统一走这个函数：自动注入 `Authorization: Bearer <token>` 头。 */
export async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  let base_url: string;
  let token: string;
  try {
    const t = await getServerToken();
    base_url = t.base_url;
    token = t.token;
  } catch (e) {
    throw new Error(`getServerToken failed: ${String(e)}`);
  }

  const url = `${base_url}${path}`;
  let resp: Response;
  try {
    resp = await fetch(url, {
      ...init,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
        ...init?.headers,
      },
    });
  } catch (e) {
    throw new Error(`fetch to ${url} failed: ${String(e)}`);
  }
  if (!resp.ok) {
    const body = await resp.text().catch(() => "");
    throw new Error(`API ${path} failed: ${resp.status} ${body}`);
  }
  return resp.json() as Promise<T>;
}
