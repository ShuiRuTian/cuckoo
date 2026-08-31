/**
 * Proxy 模式主界面（`spec.md` 6.2 节、`plan.md` M2.5）。
 *
 * 功能：
 * - 代理启停开关 + 端口配置
 * - 代理状态实时展示（running / port / flow count）
 * - SSE 事件实时订阅 → `flowMapAtom` 更新
 * - 左侧流量列表 + 右侧 Flow 详情面板（ResizablePanel 分栏）
 * - CA 证书导出按钮
 *
 * 数据流：
 * - `startProxy` / `stopProxy` → 更新 `proxyStatusAtom`
 * - SSE `subscribeFlowStream` → `TrafficEvent` → 更新 `flowMapAtom`
 * - `getProxyStatus` 轮询初始化状态
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { useAtom } from "jotai";
import { Play, Square, Download, Loader2, List, ShieldQuestion } from "lucide-react";
import { ResizablePanel } from "@/components/custom/ResizablePanel";
import { FlowList } from "./FlowList";
import { FlowDetailPanel } from "./FlowDetailPanel";
import { RuleManagerPanel } from "@/features/rules/RuleManagerPanel";
import { InterceptModal } from "@/features/rules/InterceptModal";
import {
  proxyStatusAtom,
  proxyPortAtom,
  flowMapAtom,
} from "@/state/proxy";
import {
  startProxy,
  stopProxy,
  getProxyStatus,
  exportCaCert,
} from "@/lib/api/proxy";
import { subscribeFlowStream } from "@/lib/api/flowStream";
import type { TrafficEvent, Flow } from "@/lib/api/generated";
import { cn } from "@/lib/utils";

export function ProxyDashboard() {
  const [status, setStatus] = useAtom(proxyStatusAtom);
  const [port, setPort] = useAtom(proxyPortAtom);
  const [, setFlowMap] = useAtom(flowMapAtom);
  const [toggling, setToggling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sseConnected, setSseConnected] = useState(false);
  const sseDisconnectRef = useRef<(() => void) | null>(null);

  // M3.2：视图切换（流量列表 / 规则管理）与断点模态
  const [view, setView] = useState<"flows" | "rules">("flows");
  const [pendingIntercepts, setPendingIntercepts] = useState<
    { flow_id: string; stage: string }[]
  >([]);
  // 当前展示编辑模态的断点（null = 不展示）
  const [activeIntercept, setActiveIntercept] = useState<{
    flow_id: string;
    stage: string;
  } | null>(null);

  // 新断点到达时自动弹模态（仅在没有已打开的模态时）
  useEffect(() => {
    if (!activeIntercept && pendingIntercepts.length > 0) {
      setActiveIntercept(pendingIntercepts[0]);
    }
  }, [pendingIntercepts, activeIntercept]);

  // 初始加载代理状态
  useEffect(() => {
    getProxyStatus()
      .then((s) => setStatus(s))
      .catch(() => {
        // 服务端可能未启动，忽略
      });
  }, [setStatus]);

  // 处理 SSE 事件
  const handleTrafficEvents = useCallback(
    (events: TrafficEvent[]) => {
      setFlowMap((prev) => {
        const next = new Map(prev);
        for (const event of events) {
          let flow: Flow | null = null;
          let flowId: string | null = null;

          if (event.type === "flow_started") {
            flow = event as unknown as Flow;
            flowId = flow.id;
          } else if (event.type === "flow_complete") {
            flow = event as unknown as Flow;
            flowId = event.flow_id;
          } else if (event.type === "flow_error") {
            // 标记 Flow 出错
            flowId = event.flow_id;
            const existing = next.get(flowId);
            if (existing) {
              next.set(flowId, {
                ...existing,
                status: "error",
                error: event.error,
              });
            }
            continue;
          } else if (event.type === "flow_intercepted") {
            // 标记 Flow 被拦截 + 记录挂起断点（M3.2 弹模态用）
            flowId = event.flow_id;
            const existing = next.get(flowId);
            if (existing) {
              next.set(flowId, {
                ...existing,
                status: "intercepted",
              });
            }
            setPendingIntercepts((prev) =>
              prev.some((p) => p.flow_id === event.flow_id)
                ? prev
                : [...prev, { flow_id: event.flow_id, stage: event.stage }],
            );
            continue;
          }

          if (flow && flowId) {
            next.set(flowId, flow);
          }
        }
        return next;
      });
    },
    [setFlowMap],
  );

  // 代理运行时订阅 SSE
  useEffect(() => {
    if (!status.running) {
      setSseConnected(false);
      return;
    }

    let cancelled = false;

    const setupSse = async () => {
      try {
        const disconnect = await subscribeFlowStream({
          onOpen: () => {
            if (!cancelled) setSseConnected(true);
          },
          onEvents: handleTrafficEvents,
          onError: () => {
            if (!cancelled) setSseConnected(false);
          },
          onClose: () => {
            if (!cancelled) setSseConnected(false);
          },
        });
        if (cancelled) {
          disconnect();
        } else {
          sseDisconnectRef.current = disconnect;
        }
      } catch (e) {
        if (!cancelled) {
          setError(`SSE 连接失败: ${String(e)}`);
        }
      }
    };

    setupSse();

    return () => {
      cancelled = true;
      if (sseDisconnectRef.current) {
        sseDisconnectRef.current();
        sseDisconnectRef.current = null;
      }
    };
  }, [status.running, handleTrafficEvents]);

  // 启动/停止代理
  const handleToggleProxy = useCallback(async () => {
    setToggling(true);
    setError(null);
    try {
      if (status.running) {
        const s = await stopProxy();
        setStatus(s);
      } else {
        const s = await startProxy({ port });
        setStatus(s);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setToggling(false);
    }
  }, [status.running, port, setStatus]);

  // 导出 CA 证书
  const handleExportCa = useCallback(async () => {
    setError(null);
    try {
      const result = await exportCaCert();
      // 创建下载链接（CaCertInfo.pem 为 PEM 文本；不含私钥，
      // 私钥永远不出后端）
      const blob = new Blob([result.pem], { type: "application/x-pem-file" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "cuckoo-ca-cert.pem";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      setError(`证书导出失败: ${String(e)}`);
    }
  }, []);

  // 断点决策提交后：从 pending 列表移除并刷新 Rules 面板的挂起列表
  const handleInterceptResolved = useCallback(() => {
    setPendingIntercepts((prev) =>
      prev.filter((p) => p.flow_id !== activeIntercept?.flow_id),
    );
  }, [activeIntercept]);

  const running = status.running;

  return (
    <div className="flex h-full flex-col">
      {/* 代理控制栏 */}
      <div className="flex items-center gap-3 border-b px-3 py-2">
        {/* 启停按钮 */}
        <button
          onClick={handleToggleProxy}
          disabled={toggling}
          className={cn(
            "flex items-center gap-1.5 rounded px-3 py-1.5 text-sm font-medium shadow-sm transition-colors",
            running
              ? "bg-red-600 text-white hover:bg-red-700"
              : "bg-green-600 text-white hover:bg-green-700",
            toggling && "opacity-50",
          )}
        >
          {toggling ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : running ? (
            <Square className="h-3.5 w-3.5" />
          ) : (
            <Play className="h-3.5 w-3.5" />
          )}
          {toggling ? "..." : running ? "Stop" : "Start"}
        </button>

        {/* 端口配置 */}
        <div className="flex items-center gap-1">
          <label className="text-xs text-muted-foreground">Port:</label>
          <input
            type="number"
            value={port}
            onChange={(e) => setPort(Number(e.target.value) || 8080)}
            disabled={running}
            className="w-20 rounded border border-border bg-background px-2 py-1 text-sm font-mono disabled:opacity-50"
            min={1}
            max={65535}
          />
        </div>

        {/* 状态指示 */}
        <div className="flex items-center gap-2 text-xs">
          <span
            className={cn(
              "flex items-center gap-1",
              running ? "text-green-600 dark:text-green-400" : "text-muted-foreground",
            )}
          >
            <span
              className={cn(
                "h-2 w-2 rounded-full",
                running ? "bg-green-500" : "bg-muted-foreground/40",
              )}
            />
            {running ? "Running" : "Stopped"}
          </span>
          {running && status.listen_addr && (
            <span className="font-mono text-muted-foreground">
              {status.listen_addr}
            </span>
          )}
          {running && (
            <span
              className={cn(
                "flex items-center gap-1",
                sseConnected
                  ? "text-blue-600 dark:text-blue-400"
                  : "text-muted-foreground",
              )}
            >
              <span
                className={cn(
                  "h-2 w-2 rounded-full",
                  sseConnected ? "bg-blue-500 animate-pulse" : "bg-muted-foreground/40",
                )}
              />
              {sseConnected ? "SSE Live" : "SSE..."}
            </span>
          )}
        </div>

        {/* 视图切换：流量列表 / 拦截规则（M3.2） */}
        <div className="flex items-center gap-1 rounded border border-border p-0.5">
          <button
            onClick={() => setView("flows")}
            className={cn(
              "flex items-center gap-1 rounded px-2 py-0.5 text-xs",
              view === "flows"
                ? "bg-accent font-medium"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <List className="h-3 w-3" />
            Flows
          </button>
          <button
            onClick={() => setView("rules")}
            className={cn(
              "flex items-center gap-1 rounded px-2 py-0.5 text-xs",
              view === "rules"
                ? "bg-accent font-medium"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            <ShieldQuestion className="h-3 w-3" />
            Rules
            {pendingIntercepts.length > 0 && (
              <span className="ml-0.5 rounded-full bg-yellow-500 px-1.5 text-[10px] font-semibold text-white">
                {pendingIntercepts.length}
              </span>
            )}
          </button>
        </div>

        {/* CA 证书导出 */}
        <button
          onClick={handleExportCa}
          className="ml-auto flex items-center gap-1 rounded border border-border px-2 py-1 text-xs hover:bg-accent"
          title="导出 CA 证书"
        >
          <Download className="h-3 w-3" />
          CA Cert
        </button>
      </div>

      {/* 错误提示 */}
      {error && (
        <div className="border-b bg-red-100 px-3 py-1.5 text-xs text-red-700 dark:bg-red-900/30 dark:text-red-400">
          {error}
        </div>
      )}

      {/* 主工作区：flows 视图为左流量列表 + 右详情面板；rules 视图为规则管理 */}
      {view === "flows" ? (
        <ResizablePanel
          direction="horizontal"
          className="flex-1"
          initialRatio={0.45}
        >
          <aside className="h-full overflow-hidden border-r bg-secondary/30">
            <FlowList />
          </aside>
          <main className="h-full overflow-hidden">
            <FlowDetailPanel />
          </main>
        </ResizablePanel>
      ) : (
        <div className="flex-1 overflow-hidden">
          <RuleManagerPanel
            onOpenIntercept={(flow_id, stage) =>
              setActiveIntercept({ flow_id, stage })
            }
          />
        </div>
      )}

      {/* 断点命中模态（M3.2） */}
      {activeIntercept && (
        <InterceptModal
          flowId={activeIntercept.flow_id}
          stage={activeIntercept.stage}
          onClose={() => setActiveIntercept(null)}
          onResolved={handleInterceptResolved}
        />
      )}
    </div>
  );
}
