/**
 * 流量列表组件（`spec.md` 6.2 节、`plan.md` M2.5）。
 *
 * 功能：
 * - 实时展示 SSE 推送的 Flow 事件
 * - 列：序号 / 方法 / 状态码 / Host / Path / 耗时 / 协议
 * - 点击行选中 Flow，触发详情面板加载
 * - 支持清空按钮
 *
 * 数据流：
 * - `flowMapAtom` → 转为有序数组 → 渲染行
 * - 行点击 → `selectedFlowIdAtom`
 */

import { useMemo } from "react";
import { useAtom } from "jotai";
import { Trash2 } from "lucide-react";
import { flowMapAtom, selectedFlowIdAtom, clearFlowsAtom } from "@/state/proxy";
import type { Flow, FlowStatus } from "@/lib/api/generated";
import { cn } from "@/lib/utils";

/** 状态码颜色映射 */
function statusColor(code: number | null): string {
  if (code === null) return "text-muted-foreground";
  if (code < 200) return "text-blue-600 dark:text-blue-400";
  if (code < 300) return "text-green-600 dark:text-green-400";
  if (code < 400) return "text-yellow-600 dark:text-yellow-400";
  if (code < 500) return "text-orange-600 dark:text-orange-400";
  return "text-red-600 dark:text-red-400";
}

/** 方法颜色映射 */
function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-blue-600 dark:text-blue-400";
    case "POST":
      return "text-green-600 dark:text-green-400";
    case "PUT":
    case "PATCH":
      return "text-yellow-600 dark:text-yellow-400";
    case "DELETE":
      return "text-red-600 dark:text-red-400";
    default:
      return "text-muted-foreground";
  }
}

/** 从 Flow 中提取展示信息 */
function flowSummary(flow: Flow) {
  const req = flow.request;
  const res = flow.response;

  // 从 URI 中提取 path
  let path = req.uri;
  try {
    if (req.uri.startsWith("http")) {
      const u = new URL(req.uri);
      path = u.pathname + u.search;
    } else if (req.uri.startsWith("/")) {
      // 已经是 path
    } else {
      // CONNECT host:port 格式
      path = req.uri;
    }
  } catch {
    // 保持原样
  }

  // Host: 优先从 headers 提取，其次从 server_addr
  const hostHeader = req.headers.find(
    ([k]) => k.toLowerCase() === "host",
  )?.[1];
  const host = hostHeader || flow.server_addr?.ip || "";

  const status = res?.status_code ?? null;
  const duration =
    flow.timing.end_time && flow.timing.start_time
      ? flow.timing.end_time - flow.timing.start_time
      : null;

  return {
    method: req.method || "—",
    status,
    host,
    path,
    duration,
    protocol: flow.protocol,
  };
}

/** Flow 状态指示器 */
function statusIndicator(status: FlowStatus): string {
  switch (status) {
    case "pending":
      return "⏳";
    case "complete":
      return "";
    case "error":
      return "❌";
    case "intercepted":
      return "🛑";
    default:
      return "";
  }
}

export function FlowList() {
  const [flowMap] = useAtom(flowMapAtom);
  const [selectedFlowId, setSelectedFlowId] = useAtom(selectedFlowIdAtom);
  const [, clearFlows] = useAtom(clearFlowsAtom);

  // Map → 有序数组（ULID 时间序）
  const flows = useMemo(() => Array.from(flowMap.values()), [flowMap]);

  return (
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex items-center justify-between border-b px-2 py-1">
        <span className="text-xs font-medium text-muted-foreground">
          {flows.length} flows
        </span>
        <button
          onClick={() => clearFlows()}
          className="flex items-center gap-1 rounded p-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
          title="清空列表"
        >
          <Trash2 className="h-3 w-3" />
          Clear
        </button>
      </div>

      {/* 表头 */}
      <div className="grid grid-cols-[3rem_4rem_3.5rem_1fr_2fr_4rem_4rem] gap-1 border-b px-2 py-1 text-xs font-medium text-muted-foreground">
        <span>#</span>
        <span>Method</span>
        <span>Status</span>
        <span>Host</span>
        <span>Path</span>
        <span className="text-right">Time</span>
        <span className="text-right">Proto</span>
      </div>

      {/* 列表 */}
      <div className="flex-1 overflow-auto">
        {flows.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <p className="text-xs text-muted-foreground">
              等待流量…
            </p>
          </div>
        ) : (
          flows.map((flow, idx) => {
            const s = flowSummary(flow);
            const isSelected = flow.id === selectedFlowId;
            return (
              <div
                key={flow.id}
                onClick={() => setSelectedFlowId(flow.id)}
                className={cn(
                  "grid cursor-pointer grid-cols-[3rem_4rem_3.5rem_1fr_2fr_4rem_4rem] gap-1 px-2 py-0.5 text-xs font-mono",
                  "border-b border-border/30 hover:bg-accent/50",
                  isSelected && "bg-accent text-accent-foreground",
                )}
              >
                <span className="text-muted-foreground">{idx + 1}</span>
                <span className={cn("font-medium", methodColor(s.method))}>
                  {s.method}
                </span>
                <span className={cn("font-medium", statusColor(s.status))}>
                  {statusIndicator(flow.status)} {s.status ?? "—"}
                </span>
                <span className="truncate text-foreground/80">{s.host}</span>
                <span className="truncate text-muted-foreground">{s.path}</span>
                <span className="text-right text-muted-foreground">
                  {s.duration !== null ? `${s.duration}ms` : "—"}
                </span>
                <span className="text-right text-muted-foreground">
                  {s.protocol}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
