/**
 * Flow 详情面板（`spec.md` 6.2 节、`plan.md` M2.5）。
 *
 * 功能：
 * - 展示选中 Flow 的完整 request/response 信息
 * - Tab：Summary / Request Headers / Request Body / Response Headers / Response Body / Timing
 * - Body 支持 JSON 美化
 * - Timing 瀑布图（简化版）
 *
 * 数据来源：
 * - 从 `flowMapAtom` 读取选中 Flow（实时更新）
 * - Body 惰性拉取通过 `getFlowBody` API（M2 阶段 body 内联在 Flow 中，暂不需要）
 */

import { useState, useEffect, useMemo } from "react";
import { useAtom } from "jotai";
import { Send, Save, Loader2 } from "lucide-react";
import { Modal } from "@/components/ui/Modal";
import { flowMapAtom, selectedFlowIdAtom, proxyStatusAtom } from "@/state/proxy";
import type { Flow, HttpMessage } from "@/lib/api/generated";
import {
  sendRequest,
  createRequest,
  listWorkspaces,
  listFolders,
  type WorkspaceModel,
  type FolderModel,
  type ExecutionResult,
} from "@/lib/api/generated";
import { flowToAdHoc, flowToCreateInput, flowToUrl } from "@/lib/api/flowToRequest";
import { cn } from "@/lib/utils";

type DetailTab =
  | "summary"
  | "req_headers"
  | "req_body"
  | "res_headers"
  | "res_body"
  | "timing";

/** 尝试美化 JSON body */
function prettyPrintJson(bodyBytes: number[]): string {
  const text = new TextDecoder().decode(new Uint8Array(bodyBytes));
  try {
    const parsed = JSON.parse(text);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return text;
  }
}

/** 将 body 字节转为可展示文本 */
function bodyToText(bodyBytes: number[], contentType?: string): string {
  if (bodyBytes.length === 0) return "(empty body)";
  if (contentType?.includes("json") || contentType?.includes("text")) {
    return prettyPrintJson(bodyBytes);
  }
  // 尝试 UTF-8 解码
  try {
    return new TextDecoder().decode(new Uint8Array(bodyBytes));
  } catch {
    return `(binary data, ${bodyBytes.length} bytes)`;
  }
}

/** 从消息中获取 Content-Type */
function getContentType(msg: HttpMessage | null): string | undefined {
  return msg?.headers.find(([k]) => k.toLowerCase() === "content-type")?.[1];
}

/** Headers 表格 */
function HeadersTable({ headers }: { headers: [string, string][] }) {
  if (headers.length === 0) {
    return <p className="text-xs text-muted-foreground">(no headers)</p>;
  }
  return (
    <table className="w-full text-xs">
      <thead>
        <tr className="border-b">
          <th className="px-2 py-1 text-left font-medium">Name</th>
          <th className="px-2 py-1 text-left font-medium">Value</th>
        </tr>
      </thead>
      <tbody>
        {headers.map(([name, value], i) => (
          <tr key={i} className="border-b border-border/30">
            <td className="px-2 py-1 font-mono text-foreground/80">{name}</td>
            <td className="px-2 py-1 font-mono break-all text-muted-foreground">
              {value}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** Body 展示 */
function BodyViewer({
  msg,
  label,
}: {
  msg: HttpMessage | null;
  label: string;
}) {
  if (!msg) {
    return (
      <p className="text-xs text-muted-foreground">{label} 不可用</p>
    );
  }
  const contentType = getContentType(msg);
  const text = bodyToText(msg.body, contentType);
  const truncated = msg.body_truncated;
  const actualSize = msg.body_size;

  return (
    <div className="flex h-full flex-col gap-1">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <span>{contentType || "unknown content-type"}</span>
        <span>{actualSize} bytes</span>
        {truncated && (
          <span className="text-yellow-600 dark:text-yellow-400">
            (truncated)
          </span>
        )}
      </div>
      <pre className="flex-1 overflow-auto rounded bg-muted/50 p-2 font-mono text-xs">
        {text}
      </pre>
    </div>
  );
}

/** Timing 瀑布图（简化版） */
function TimingView({ flow }: { flow: Flow }) {
  const t = flow.timing;
  const stages: { label: string; start: number | null; end: number | null }[] = [
    { label: "DNS", start: t.dns_start, end: t.dns_end },
    { label: "TCP", start: t.connect_start, end: t.connect_end },
    { label: "TLS", start: t.tls_start, end: t.tls_end },
    { label: "Send", start: t.send_start, end: t.send_end },
    { label: "TTFB", start: t.send_end, end: t.ttfb },
    { label: "Download", start: t.ttfb, end: t.end_time },
  ];

  const totalTime = t.end_time
    ? t.end_time - t.start_time
    : 0;
  const hasTiming = stages.some((s) => s.start !== null && s.end !== null);

  if (!hasTiming) {
    return (
      <p className="text-xs text-muted-foreground">
        (timing data unavailable)
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="text-xs text-muted-foreground">
        Total: {totalTime}ms
      </div>
      {stages.map((stage) => {
        if (stage.start === null || stage.end === null) return null;
        const offset = stage.start - t.start_time;
        const duration = stage.end - stage.start;
        const leftPct = totalTime > 0 ? (offset / totalTime) * 100 : 0;
        const widthPct = totalTime > 0 ? (duration / totalTime) * 100 : 0;
        return (
          <div key={stage.label} className="flex items-center gap-2 text-xs">
            <span className="w-16 text-muted-foreground">{stage.label}</span>
            <div className="relative h-4 flex-1 rounded bg-muted/30">
              <div
                className="absolute h-full rounded bg-primary/40"
                style={{
                  left: `${leftPct}%`,
                  width: `${Math.max(widthPct, 0.5)}%`,
                }}
              />
            </div>
            <span className="w-16 text-right font-mono text-muted-foreground">
              {duration}ms
            </span>
          </div>
        );
      })}
    </div>
  );
}

export function FlowDetailPanel() {
  const [flowMap] = useAtom(flowMapAtom);
  const [selectedFlowId] = useAtom(selectedFlowIdAtom);
  const [proxyStatus] = useAtom(proxyStatusAtom);
  const [tab, setTab] = useState<DetailTab>("summary");

  // M3.3 联动：重发 / 另存为 Collection 请求
  const [resending, setResending] = useState(false);
  const [replayResult, setReplayResult] = useState<ExecutionResult | null>(null);
  const [saveOpen, setSaveOpen] = useState(false);

  const flow: Flow | null = selectedFlowId
    ? flowMap.get(selectedFlowId) ?? null
    : null;

  // 重发：用 ad-hoc 请求直接重放（M3.3）。
  // 代理运行中时经本地代理转发（via_proxy）：请求会被拦截规则处理，
  // 并作为新 Flow 出现在流量列表；代理未运行则直连。
  const handleReplay = async () => {
    if (!flow) return;
    setResending(true);
    setReplayResult(null);
    try {
      const result = await sendRequest({
        request_id: null,
        ad_hoc: flowToAdHoc(flow),
        environment_id: null,
        via_proxy: proxyStatus.running,
      });
      setReplayResult(result);
    } catch (e) {
      setReplayResult({
        status: 0,
        status_text: "Replay Failed",
        headers: {},
        body: String(e),
        body_size: 0,
        content_type: null,
        total_time_ms: 0,
        success: false,
        error: String(e),
      });
    } finally {
      setResending(false);
    }
  };

  // 自动切换到 summary tab 当 Flow 变化时
  const flowId = flow?.id;
  const [lastFlowId, setLastFlowId] = useState<string | null>(null);
  if (flowId && flowId !== lastFlowId) {
    setLastFlowId(flowId);
    if (tab === "res_headers" || tab === "res_body") {
      if (!flow?.response) setTab("summary");
    }
  }

  const tabs = useMemo(() => {
    if (!flow) return [] as { key: DetailTab; label: string }[];
    const list: { key: DetailTab; label: string }[] = [
      { key: "summary", label: "Summary" },
      { key: "req_headers", label: "Req Headers" },
      { key: "req_body", label: "Req Body" },
    ];
    if (flow.response) {
      list.push({ key: "res_headers", label: "Res Headers" });
      list.push({ key: "res_body", label: "Res Body" });
    }
    list.push({ key: "timing", label: "Timing" });
    return list;
  }, [flow]);

  if (!flow) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">
          选择左侧流量行查看详情
        </p>
      </div>
    );
  }

  const req = flow.request;
  const res = flow.response;

  return (
    <div className="flex h-full flex-col">
      {/* 联动操作栏（M3.3）：重发 / 另存为 Collection 请求 */}
      <div className="flex items-center gap-2 border-b px-2 py-1.5">
        <button
          onClick={handleReplay}
          disabled={resending}
          className="flex items-center gap-1 rounded border border-border px-2 py-1 text-xs hover:bg-accent disabled:opacity-50"
          title={
            proxyStatus.running
              ? "经本地代理重放（会被拦截规则处理并出现在流量列表）"
              : "直连重放（启动代理后重放可被拦截规则处理）"
          }
        >
          {resending ? (
            <Loader2 className="h-3 w-3 animate-spin" />
          ) : (
            <Send className="h-3 w-3" />
          )}
          重发
        </button>
        <button
          onClick={() => setSaveOpen(true)}
          className="flex items-center gap-1 rounded border border-border px-2 py-1 text-xs hover:bg-accent"
          title="把此请求保存到 Collection"
        >
          <Save className="h-3 w-3" />
          另存为请求
        </button>

        {replayResult && (
          <span
            className={cn(
              "ml-2 rounded px-2 py-0.5 text-xs font-mono",
              replayResult.success
                ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
            )}
            title={replayResult.error ?? ""}
          >
            {replayResult.status} · {replayResult.total_time_ms}ms
          </span>
        )}
      </div>

      {/* Tab 栏 */}
      <div className="flex border-b">
        {tabs.map(({ key, label }) => (
          <button
            key={key}
            onClick={() => setTab(key)}
            className={cn(
              "border-b-2 px-3 py-1.5 text-sm",
              tab === key
                ? "border-primary text-primary"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {label}
          </button>
        ))}
      </div>

      {/* Tab 内容 */}
      <div className="flex-1 overflow-auto p-2">
        {tab === "summary" && (
          <div className="flex flex-col gap-2 text-xs">
            <div className="grid grid-cols-[8rem_1fr] gap-1">
              <span className="text-muted-foreground">Flow ID</span>
              <span className="font-mono">{flow.id}</span>

              <span className="text-muted-foreground">Protocol</span>
              <span>{flow.protocol}</span>

              <span className="text-muted-foreground">Status</span>
              <span>{flow.status}</span>

              <span className="text-muted-foreground">Method</span>
              <span className="font-mono">{req.method}</span>

              <span className="text-muted-foreground">URI</span>
              <span className="font-mono break-all">{req.uri}</span>

              <span className="text-muted-foreground">Start Line</span>
              <span className="font-mono">{req.start_line}</span>

              {flow.server_addr && (
                <>
                  <span className="text-muted-foreground">Server</span>
                  <span className="font-mono">
                    {flow.server_addr.ip}:{flow.server_addr.port}
                  </span>
                </>
              )}

              {flow.client_addr && (
                <>
                  <span className="text-muted-foreground">Client</span>
                  <span className="font-mono">
                    {flow.client_addr.ip}:{flow.client_addr.port}
                  </span>
                </>
              )}

              {flow.tls && (
                <>
                  <span className="text-muted-foreground">TLS Version</span>
                  <span>{flow.tls.version}</span>

                  <span className="text-muted-foreground">SNI</span>
                  <span>{flow.tls.sni || "—"}</span>

                  <span className="text-muted-foreground">ALPN</span>
                  <span>{flow.tls.alpn || "—"}</span>
                </>
              )}

              {res && (
                <>
                  <span className="text-muted-foreground">Status Code</span>
                  <span className="font-mono">
                    {res.status_code} {res.start_line}
                  </span>
                </>
              )}

              {flow.error && (
                <>
                  <span className="text-muted-foreground">Error</span>
                  <span className="text-red-600 dark:text-red-400">
                    {flow.error}
                  </span>
                </>
              )}
            </div>
          </div>
        )}

        {tab === "req_headers" && <HeadersTable headers={req.headers} />}

        {tab === "req_body" && (
          <BodyViewer msg={req} label="Request body" />
        )}

        {tab === "res_headers" && res && (
          <HeadersTable headers={res.headers} />
        )}

        {tab === "res_body" && res && (
          <BodyViewer msg={res} label="Response body" />
        )}

        {tab === "timing" && <TimingView flow={flow} />}
      </div>

      {/* 另存为 Collection 请求模态（M3.3） */}
      {flow && (
        <SaveFlowModal
          flow={flow}
          open={saveOpen}
          onClose={() => setSaveOpen(false)}
        />
      )}
    </div>
  );
}

// ────────────────────────────────────────────────────────────────────
// 另存为 Collection 请求模态（M3.3）
// ────────────────────────────────────────────────────────────────────

function SaveFlowModal({
  flow,
  open,
  onClose,
}: {
  flow: Flow;
  open: boolean;
  onClose: () => void;
}) {
  const [workspaces, setWorkspaces] = useState<WorkspaceModel[]>([]);
  const [folders, setFolders] = useState<FolderModel[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [folderId, setFolderId] = useState<string>("");
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // 加载 workspace 列表（模态打开时）
  useEffect(() => {
    if (!open) return;
    listWorkspaces()
      .then((ws) => {
        setWorkspaces(ws);
        if (ws.length > 0) {
          setWorkspaceId(ws[0].id);
          return listFolders(ws[0].id).then(setFolders);
        }
      })
      .catch((e) => setError(String(e)));
  }, [open]);

  const handleWorkspaceChange = (id: string) => {
    setWorkspaceId(id);
    setFolderId("");
    listFolders(id).then(setFolders).catch(() => setFolders([]));
  };

  const handleSave = async () => {
    if (!workspaceId) {
      setError("请选择 Workspace");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const input = flowToCreateInput(flow, workspaceId, folderId || null);
      if (name.trim()) input.name = name.trim();
      await createRequest(input);
      setSaved(true);
      setTimeout(() => {
        setSaved(false);
        onClose();
      }, 800);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      width="max-w-md"
      title="另存为 Collection 请求"
      footer={
        <>
          {error && (
            <span className="mr-auto max-w-56 truncate text-xs text-red-600 dark:text-red-400" title={error}>
              {error}
            </span>
          )}
          <button
            onClick={onClose}
            className="rounded border border-border px-3 py-1.5 text-sm hover:bg-accent"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={saving || saved}
            className="rounded bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {saved ? "已保存 ✓" : saving ? "保存中..." : "保存"}
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">名称</span>
          <input
            className="rounded border border-border bg-background px-2 py-1 text-sm font-mono"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={flowToCreateInput(flow, "", null).name}
          />
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">Workspace</span>
          <select
            className="rounded border border-border bg-background px-2 py-1 text-sm"
            value={workspaceId}
            onChange={(e) => handleWorkspaceChange(e.target.value)}
          >
            {workspaces.length === 0 && <option value="">（无 Workspace）</option>}
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>
                {w.name}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">
            Folder（可选）
          </span>
          <select
            className="rounded border border-border bg-background px-2 py-1 text-sm"
            value={folderId}
            onChange={(e) => setFolderId(e.target.value)}
          >
            <option value="">（根目录）</option>
            {folders.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        </label>

        <div className="rounded bg-muted/40 p-2 text-xs text-muted-foreground">
          <div className="font-mono">{flow.request.method} {flowToUrl(flow)}</div>
        </div>
      </div>
    </Modal>
  );
}
