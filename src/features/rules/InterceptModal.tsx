/**
 * 断点命中模态编辑界面（`plan.md` M3.2、`spec.md` 4.5 节）。
 *
 * 触发：SSE `flow_intercepted` 事件（ProxyDashboard 监听后设置 pendingInterceptAtom）。
 *
 * 功能：
 * - 展示被挂起的原始 request / response
 * - Headers 键值对编辑（增/删/改）
 * - Body 文本编辑
 * - 三个决策按钮：
 *   - 放行（含修改）→ InterceptDecision::Continue { edited }
 *   - 丢弃（返回空响应）→ InterceptDecision::Abort
 *   - 中断连接 → InterceptDecision::DropConnection
 */

import { useEffect, useState } from "react";
import { Modal } from "@/components/ui/Modal";
import {
  getIntercept,
  resumeInterceptedFlow,
  type InterceptDecision,
  type PendingInterceptDetail,
  type ProxyHttpMessage,
} from "@/lib/api/generated";
import { cn } from "@/lib/utils";

const inputCls =
  "rounded border border-border bg-background px-2 py-1 text-sm font-mono";

/** body 字节数组 ↔ 文本 */
function bodyToText(body: number[]): string {
  if (body.length === 0) return "";
  try {
    return new TextDecoder().decode(new Uint8Array(body));
  } catch {
    return "";
  }
}

function textToBody(text: string): number[] {
  return Array.from(new TextEncoder().encode(text));
}

// ────────────────────────────────────────────────────────────────────
// Headers 编辑器
// ────────────────────────────────────────────────────────────────────

function HeadersEditor({
  headers,
  onChange,
}: {
  headers: [string, string][];
  onChange: (headers: [string, string][]) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      {headers.map(([k, v], i) => (
        <div key={i} className="flex items-center gap-1">
          <input
            className={cn(inputCls, "w-44")}
            value={k}
            onChange={(e) => {
              const next = [...headers];
              next[i] = [e.target.value, v];
              onChange(next);
            }}
          />
          <input
            className={cn(inputCls, "flex-1")}
            value={v}
            onChange={(e) => {
              const next = [...headers];
              next[i] = [k, e.target.value];
              onChange(next);
            }}
          />
          <button
            onClick={() => onChange(headers.filter((_, j) => j !== i))}
            className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-red-500"
            title="删除"
          >
            ✕
          </button>
        </div>
      ))}
      <button
        onClick={() => onChange([...headers, ["", ""]])}
        className="self-start rounded border border-dashed border-border px-2 py-0.5 text-xs text-muted-foreground hover:bg-accent"
      >
        + Header
      </button>
    </div>
  );
}

// ────────────────────────────────────────────────────────────────────
// 主组件
// ────────────────────────────────────────────────────────────────────

export function InterceptModal({
  flowId,
  stage,
  onClose,
  onResolved,
}: {
  /** 被挂起的 flow ID */
  flowId: string;
  /** SSE 事件携带的阶段信息（"request" / "response"） */
  stage: string;
  onClose: () => void;
  /** 决策提交成功后回调（刷新 pending 列表） */
  onResolved: () => void;
}) {
  const [detail, setDetail] = useState<PendingInterceptDetail | null>(null);
  const [msg, setMsg] = useState<ProxyHttpMessage | null>(null);
  const [bodyText, setBodyText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState<InterceptDecision["decision"] | null>(null);

  // 加载断点详情（原始消息）
  useEffect(() => {
    let cancelled = false;
    getIntercept(flowId)
      .then((d) => {
        if (cancelled) return;
        setDetail(d);
        setMsg(d.original);
        setBodyText(bodyToText(d.original.body));
      })
      .catch((e) => {
        if (cancelled) return;
        // 可能已被其他客户端处理或超时消失
        setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [flowId]);

  const submit = async (decision: InterceptDecision) => {
    setSubmitting(decision.decision);
    setError(null);
    try {
      await resumeInterceptedFlow(flowId, decision);
      onResolved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSubmitting(null);
    }
  };

  /** 构造编辑后的消息（放行时使用） */
  const buildEdited = (): ProxyHttpMessage => {
    if (!msg) throw new Error("message not loaded");
    const body = textToBody(bodyText);
    // 同步 Content-Length
    const headers = [...msg.headers];
    const clIdx = headers.findIndex(([k]) => k.toLowerCase() === "content-length");
    if (clIdx >= 0) {
      headers[clIdx] = ["Content-Length", String(body.length)];
    }
    return { ...msg, headers, body };
  };

  const isRequestStage = (detail?.stage ?? stage) === "request";

  return (
    <Modal
      open
      onClose={onClose}
      closeOnOverlay={false}
      width="max-w-3xl"
      title={
        <span className="flex items-center gap-2">
          <span className="inline-block h-2 w-2 animate-pulse rounded-full bg-yellow-500" />
          断点命中 — {isRequestStage ? "请求" : "响应"}阶段挂起
          <code className="rounded bg-muted px-1.5 py-0.5 text-xs">{flowId}</code>
        </span>
      }
      footer={
        <>
          {error && (
            <span className="mr-auto max-w-md truncate text-xs text-red-600 dark:text-red-400" title={error}>
              {error}
            </span>
          )}
          <button
            onClick={onClose}
            className="rounded border border-border px-3 py-1.5 text-sm hover:bg-accent"
            title="稍后处理（Flow 保持挂起）"
          >
            稍后处理
          </button>
          <button
            onClick={() => submit({ decision: "drop_connection" })}
            disabled={submitting !== null}
            className="rounded bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
          >
            {submitting === "drop_connection" ? "..." : "中断连接"}
          </button>
          <button
            onClick={() => submit({ decision: "abort" })}
            disabled={submitting !== null}
            className="rounded bg-orange-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-orange-700 disabled:opacity-50"
          >
            {submitting === "abort" ? "..." : "丢弃请求"}
          </button>
          <button
            onClick={() =>
              submit({ decision: "continue", edited: buildEdited() })
            }
            disabled={submitting !== null || !msg}
            className="rounded bg-green-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-green-700 disabled:opacity-50"
          >
            {submitting === "continue" ? "..." : "放行（含修改）"}
          </button>
        </>
      }
    >
      {!detail && !error && <p className="text-xs text-muted-foreground">加载断点详情...</p>}

      {error && !detail && (
        <p className="text-xs text-muted-foreground">
          断点已不存在（可能被其他客户端处理）：{error}
        </p>
      )}

      {msg && (
        <div className="flex flex-col gap-4">
          {/* 起始行信息（只读） */}
          <div className="grid grid-cols-[5rem_1fr] gap-x-3 gap-y-1 rounded bg-muted/40 p-2 text-xs">
            <span className="text-muted-foreground">Method</span>
            <code>{isRequestStage ? msg.method : "—"}</code>
            <span className="text-muted-foreground">URI</span>
            <code className="break-all">{isRequestStage ? msg.uri : "—"}</code>
            <span className="text-muted-foreground">Version</span>
            <code>{msg.version}</code>
          </div>

          {/* Headers 编辑 */}
          <div>
            <h4 className="mb-1.5 text-xs font-semibold">Headers</h4>
            <HeadersEditor
              headers={msg.headers}
              onChange={(headers) => setMsg({ ...msg, headers })}
            />
          </div>

          {/* Body 编辑 */}
          <div>
            <h4 className="mb-1.5 text-xs font-semibold">
              Body（{msg.body.length} bytes）
            </h4>
            <textarea
              className={cn(inputCls, "min-h-32 w-full")}
              value={bodyText}
              onChange={(e) => setBodyText(e.target.value)}
              placeholder="body 内容（二进制不可编辑）"
            />
          </div>
        </div>
      )}
    </Modal>
  );
}
