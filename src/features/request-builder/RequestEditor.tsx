/**
 * 请求编辑器（`spec.md` 6.2 节、`plan.md` M1.4）。
 *
 * M1 阶段实现：
 * - URL 栏 + Method 选择器
 * - Params / Headers / Body(Raw) Tab
 * - Send 按钮：通过 `sendRequest` API 发送 ad-hoc 请求
 *
 * 数据流：
 * - 选中 Collection 树中的 Request → 加载到编辑器
 * - 编辑器修改后可 "Save"（updateRequest）或直接 "Send"（sendRequest with ad_hoc）
 */

import { useState, useEffect, useCallback } from "react";
import { useAtom } from "jotai";
import { useQuery, useMutation } from "@tanstack/react-query";
import { Send, Save } from "lucide-react";
import {
  getRequest,
  updateRequest,
  sendRequest,
  type HeaderEntry,
  type KeyValueEntry,
  type RequestBody,
  type AuthConfig,
  type ExecutionResult,
  type UpdateRequestInput,
  type SendRequestInput,
} from "@/lib/api/generated";
import { KeyValueEditor } from "@/components/custom/KeyValueEditor";
import { selectedRequestIdAtom, currentEnvironmentIdAtom } from "@/state/app";
import { cn } from "@/lib/utils";
import { ResponseViewer } from "@/features/request-builder/ResponseViewer";

const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] as const;

type Tab = "params" | "headers" | "body";

/** 请求编辑器的本地状态（从 loaded request 初始化） */
interface RequestEditorState {
  method: string;
  url: string;
  headers: HeaderEntry[];
  queryParams: KeyValueEntry[];
  body: RequestBody;
  auth: AuthConfig;
}

const DEFAULT_STATE: RequestEditorState = {
  method: "GET",
  url: "",
  headers: [],
  queryParams: [],
  body: { type: "none" },
  auth: { type: "none" },
};

export function RequestEditor() {
  const [selectedRequestId] = useAtom(selectedRequestIdAtom);
  const [environmentId] = useAtom(currentEnvironmentIdAtom);
  const [tab, setTab] = useState<Tab>("params");
  const [state, setState] = useState<RequestEditorState>(DEFAULT_STATE);
  const [response, setResponse] = useState<ExecutionResult | null>(null);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // M3.3：经本地代理转发（需先在 Proxy 模式启动代理）
  const [viaProxy, setViaProxy] = useState(false);

  // 加载选中的请求
  const requestQuery = useQuery({
    queryKey: ["request", selectedRequestId],
    queryFn: () => getRequest(selectedRequestId!),
    enabled: !!selectedRequestId,
  });

  // 当请求加载完成时，初始化编辑器状态
  useEffect(() => {
    if (requestQuery.data) {
      const r = requestQuery.data;
      setState({
        method: r.method,
        url: r.url,
        headers: r.headers,
        queryParams: r.query_params,
        body: r.body,
        auth: r.auth,
      });
    } else if (!selectedRequestId) {
      setState(DEFAULT_STATE);
    }
  }, [requestQuery.data, selectedRequestId]);

  // 保存请求
  const updateMut = useMutation({
    mutationFn: (input: UpdateRequestInput) => updateRequest(selectedRequestId!, input),
    onSuccess: () => {
      setError(null);
    },
    onError: (e: Error) => setError(e.message),
  });

  const handleSave = useCallback(() => {
    if (!selectedRequestId) return;
    updateMut.mutate({
      folder_id: null,
      name: null,
      method: state.method,
      url: state.url,
      headers: state.headers,
      query_params: state.queryParams,
      body: state.body,
      auth: state.auth,
      sort_key: null,
    });
  }, [selectedRequestId, state, updateMut]);

  // 发送请求
  const handleSend = useCallback(async () => {
    setSending(true);
    setError(null);
    setResponse(null);
    try {
      const input: SendRequestInput = selectedRequestId
        ? { request_id: selectedRequestId, ad_hoc: null, environment_id: environmentId, via_proxy: viaProxy || null }
        : {
            request_id: null,
            ad_hoc: {
              method: state.method,
              url: state.url,
              headers: state.headers,
              query_params: state.queryParams,
              body: state.body,
              auth: state.auth,
            },
            environment_id: environmentId,
            via_proxy: viaProxy || null,
          };
      const result = await sendRequest(input);
      setResponse(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setSending(false);
    }
  }, [selectedRequestId, state, environmentId, viaProxy]);

  const updateField = <K extends keyof RequestEditorState>(
    key: K,
    value: RequestEditorState[K],
  ) => {
    setState((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <div className="flex h-full flex-col">
      {/* URL 栏 + Method + Send */}
      <div className="flex items-center gap-2 border-b p-2">
        <select
          value={state.method}
          onChange={(e) => updateField("method", e.target.value)}
          className="rounded border border-border bg-background px-2 py-1 text-sm font-medium"
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={state.url}
          onChange={(e) => updateField("url", e.target.value)}
          placeholder="https://httpbin.org/get 或 {{baseUrl}}/get"
          className="flex-1 rounded border border-border bg-background px-2 py-1 text-sm"
        />
        <button
          onClick={handleSend}
          disabled={sending || !state.url}
          className="flex items-center gap-1 rounded bg-primary px-3 py-1 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        >
          <Send className="h-3.5 w-3.5" />
          {sending ? "Sending…" : "Send"}
        </button>
        {selectedRequestId && (
          <button
            onClick={handleSave}
            disabled={updateMut.isPending}
            className="flex items-center gap-1 rounded border border-border px-3 py-1 text-sm font-medium hover:bg-accent"
          >
            <Save className="h-3.5 w-3.5" />
            Save
          </button>
        )}
        {/* M3.3：经本地代理转发开关 */}
        <label
          className="flex items-center gap-1 text-xs text-muted-foreground"
          title="请求经过本地 MITM 代理转发，可被拦截规则处理并在流量列表中查看（需先启动代理）"
        >
          <input
            type="checkbox"
            checked={viaProxy}
            onChange={(e) => setViaProxy(e.target.checked)}
          />
          经代理
        </label>
      </div>

      {/* Tab 栏 */}
      <div className="flex border-b">
        {(
          [
            ["params", "Params"],
            ["headers", "Headers"],
            ["body", "Body"],
          ] as const
        ).map(([key, label]) => (
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
        {tab === "params" && (
          <KeyValueEditor<KeyValueEntry>
            entries={state.queryParams}
            toEntries={(items) => items.map((e) => ({ key: e.key, value: e.value, enabled: e.enabled }))}
            fromEntries={(entries) => entries.map((e) => ({ key: e.key, value: e.value, enabled: e.enabled }))}
            onChange={(entries) => updateField("queryParams", entries)}
            keyPlaceholder="param"
            valuePlaceholder="value"
          />
        )}
        {tab === "headers" && (
          <KeyValueEditor<HeaderEntry>
            entries={state.headers}
            toEntries={(items) => items.map((e) => ({ key: e.name, value: e.value, enabled: e.enabled }))}
            fromEntries={(entries) => entries.map((e) => ({ name: e.key, value: e.value, enabled: e.enabled }))}
            onChange={(entries) => updateField("headers", entries)}
            keyPlaceholder="header name"
            valuePlaceholder="header value"
          />
        )}
        {tab === "body" && (
          <BodyEditor
            body={state.body}
            onChange={(body) => updateField("body", body)}
          />
        )}
      </div>

      {/* 错误提示 */}
      {error && (
        <div className="border-t bg-red-100 p-2 text-xs text-red-700 dark:bg-red-900/30 dark:text-red-400">
          {error}
        </div>
      )}

      {/* 响应查看器 */}
      {response && (
        <div className="h-[40%] border-t">
          <ResponseViewer result={response} />
        </div>
      )}
    </div>
  );
}

/** Body 编辑器子组件 */
function BodyEditor({
  body,
  onChange,
}: {
  body: RequestBody;
  onChange: (body: RequestBody) => void;
}) {
  if (body.type === "none") {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-xs text-muted-foreground">当前无 Body</p>
        <button
          onClick={() =>
            onChange({ type: "raw", content_type: "application/json", text: "{}" })
          }
          className="w-fit rounded border border-border px-2 py-1 text-xs hover:bg-accent"
        >
          + Raw Body
        </button>
      </div>
    );
  }

  // type === "raw"
  return (
    <div className="flex h-full flex-col gap-2">
      <div className="flex items-center gap-2">
        <select
          value={body.content_type}
          onChange={(e) =>
            onChange({ ...body, content_type: e.target.value })
          }
          className="rounded border border-border bg-background px-2 py-1 text-xs"
        >
          <option value="application/json">application/json</option>
          <option value="text/plain">text/plain</option>
          <option value="application/xml">application/xml</option>
          <option value="text/html">text/html</option>
        </select>
        <button
          onClick={() => onChange({ type: "none" })}
          className="rounded border border-border px-2 py-1 text-xs hover:bg-accent"
        >
          Remove
        </button>
      </div>
      <textarea
        value={body.text}
        onChange={(e) => onChange({ ...body, text: e.target.value })}
        className="flex-1 resize-none rounded border border-border bg-background p-2 font-mono text-xs"
        placeholder='{"key": "value"}'
      />
    </div>
  );
}
