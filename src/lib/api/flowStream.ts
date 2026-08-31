/**
 * Flow 事件 SSE 订阅封装（`spec.md` 6.3 节，`plan.md` M2.3 节）。
 *
 * 封装 `EventSource` 订阅 `/api/flows/stream` 端点的逻辑：
 * - 自动从 `getServerToken()` 获取 token 和 base URL
 * - 解析 `flow.batch` SSE 事件为 `TrafficEvent[]`
 * - 提供回调式 API 和自动重连提示
 *
 * 浏览器 `EventSource` 不支持自定义请求头，token 通过 `?token=` query 参数传递
 * （`spec.md` 7.5 节）。
 */

import type { TrafficEvent } from "./generated/types";
import { getServerToken } from "./token";

export interface FlowStreamOptions {
  /** 收到批量事件时的回调 */
  onEvents: (events: TrafficEvent[]) => void;
  /** 连接出错时的回调 */
  onError?: (error: Event) => void;
  /** 连接打开时的回调 */
  onOpen?: () => void;
  /** 连接关闭时的回调 */
  onClose?: () => void;
}

/**
 * 订阅 Flow 事件流。
 *
 * 返回一个 `disconnect()` 函数，调用后关闭 EventSource 连接。
 *
 * @example
 * ```ts
 * const disconnect = subscribeFlowStream({
 *   onEvents: (events) => {
 *     for (const event of events) {
 *       console.log("flow event:", event);
 *     }
 *   },
 *   onError: () => {
 *     console.warn("SSE connection lost, will auto-reconnect");
 *   },
 * });
 *
 * // 断开连接
 * disconnect();
 * ```
 */
export async function subscribeFlowStream(
  options: FlowStreamOptions,
): Promise<() => void> {
  const { base_url, token } = await getServerToken();
  const url = `${base_url}/api/flows/stream?token=${encodeURIComponent(token)}`;

  let eventSource: EventSource | null = null;
  let closed = false;

  const connect = () => {
    if (closed) return;

    eventSource = new EventSource(url);

    eventSource.onopen = () => {
      options.onOpen?.();
    };

    eventSource.addEventListener("flow.batch", (e: MessageEvent) => {
      try {
        const events: TrafficEvent[] = JSON.parse(e.data);
        options.onEvents(events);
      } catch (err) {
        console.error("Failed to parse flow.batch event:", err);
      }
    });

    eventSource.addEventListener("flow.end", (e: MessageEvent) => {
      console.info("Flow stream ended:", e.data);
      options.onClose?.();
    });

    eventSource.onerror = (e) => {
      options.onError?.(e);
      // EventSource 会自动重连，不需要手动处理
      // 如果服务端关闭，EventSource 的 readyState 会变为 CLOSED
      if (eventSource?.readyState === EventSource.CLOSED) {
        options.onClose?.();
      }
    };
  };

  connect();

  // 返回断开连接函数
  return () => {
    closed = true;
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
  };
}
