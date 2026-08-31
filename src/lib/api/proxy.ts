/**
 * Flow 查询相关 REST API 封装。
 *
 * `/api/flows` 系列端点在 `cuckoo-server` 中是手写 axum handler
 * （非 `#[rpc_method]` 标注），不在 `generated/api.ts` 的生成范围内，
 * 因此在此手写封装；类型统一从 `generated/types.ts` 导入
 * （ts-rs 生成，单一真源）。
 *
 * 代理启停与 CA 导出属于 RPC 生成范围，直接 re-export 生成版本，
 * 避免两份实现漂移。
 */

import { apiFetch } from "./client";
import type { Flow, FlowBodyResponse, FlowListResponse } from "./generated/types";

export {
  startProxy,
  stopProxy,
  getProxyStatus,
  exportCaCert,
} from "./generated/api";

export type {
  CaCertInfo,
  ProxyStatus,
  StartProxyInput,
} from "./generated/types";

/** GET /api/flows — 查询 Flow 列表 */
export function listFlows(
  limit?: number,
  offset?: number,
  host?: string,
): Promise<FlowListResponse> {
  const params = new URLSearchParams();
  if (limit !== undefined) params.set("limit", String(limit));
  if (offset !== undefined) params.set("offset", String(offset));
  if (host) params.set("host", host);
  const qs = params.toString();
  return apiFetch<FlowListResponse>(`/api/flows${qs ? `?${qs}` : ""}`, {
    method: "GET",
  });
}

/** GET /api/flows/:id — 单条 Flow 详情 */
export function getFlow(flowId: string): Promise<Flow> {
  return apiFetch<Flow>(`/api/flows/${encodeURIComponent(flowId)}`, {
    method: "GET",
  });
}

/** GET /api/flows/:id/body?part=request|response — 惰性拉取 body */
export function getFlowBody(
  flowId: string,
  part: "request" | "response",
): Promise<FlowBodyResponse> {
  return apiFetch<FlowBodyResponse>(
    `/api/flows/${encodeURIComponent(flowId)}/body?part=${part}`,
    { method: "GET" },
  );
}
