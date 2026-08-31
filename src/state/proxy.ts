/**
 * Proxy 模式相关 Jotai atoms（`spec.md` 6.1 节、6.2 节）。
 *
 * 管理：
 * - 代理运行状态（running / port / listen_addr）
 * - 当前选中的 Flow ID（流量列表点击行）
 * - 流量列表数据（由 SSE 事件实时更新）
 */

import { atom } from "jotai";
import type { Flow, ProxyStatus } from "@/lib/api/generated";

/** 代理状态 */
export const proxyStatusAtom = atom<ProxyStatus>({
  running: false,
  listen_addr: null,
  port: null,
  flow_count: 0,
});

/** 代理监听端口（用户输入） */
export const proxyPortAtom = atom<number>(8080);

/** 当前选中的 Flow ID */
export const selectedFlowIdAtom = atom<string | null>(null);

/**
 * Flow 列表（Map<flowId, Flow>），由 SSE 事件实时更新。
 *
 * 使用 Map 而非数组，方便 O(1) 查找与更新。
 * 列表渲染时按插入顺序（即 ULID 时间顺序）展示。
 */
export const flowMapAtom = atom<Map<string, Flow>>(new Map());

/** 清空所有 Flow */
export const clearFlowsAtom = atom(null, (_get, set) => {
  set(flowMapAtom, new Map());
  set(selectedFlowIdAtom, null);
});
