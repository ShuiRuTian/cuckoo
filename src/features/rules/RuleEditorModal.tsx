/**
 * 规则新建/编辑模态框（`plan.md` M3.2）。
 *
 * 支持 6 种规则类型（对应 `InterceptRule` 枚举）：
 * - Breakpoint：请求/响应阶段断点
 * - MapLocal：映射到本地内容（短路返回）
 * - MapRemote：映射到远端 URL
 * - Rewrite：headers/body 改写（含 diff 预览）
 * - Block：阻断（短路返回状态码）
 * - Throttle/Delay：延迟
 *
 * Matcher 通用字段：host glob / path glob / method / enabled
 */

import { useState } from "react";
import { Modal } from "@/components/ui/Modal";
import { RewriteDiffView } from "./RewriteDiffView";
import {
  createRule,
  updateRule,
  type InterceptRule,
  type RewriteOp,
  type RuleEntry,
} from "@/lib/api/generated";
import { cn } from "@/lib/utils";

type RuleType = InterceptRule["rule_type"];

const RULE_TYPE_LABELS: Record<RuleType, string> = {
  breakpoint: "断点 Breakpoint",
  map_local: "Map Local",
  map_remote: "Map Remote",
  rewrite: "Rewrite 重写",
  block: "Block 阻断",
  throttle_or_delay: "延迟 Throttle",
};

/** 示例消息（Rewrite diff 预览用） */
const SAMPLE_MESSAGE = {
  method: "GET",
  uri: "https://api.example.com/v1/users",
  version: "HTTP/1.1",
  headers: [
    ["Host", "api.example.com"],
    ["User-Agent", "cuckoo/0.1"],
    ["Accept", "application/json"],
  ] as [string, string][],
  body: Array.from(new TextEncoder().encode('{"page":1,"items":[]}')),
};

// ────────────────────────────────────────────────────────────────────
// 表单状态类型
// ────────────────────────────────────────────────────────────────────

interface MatcherForm {
  host_pattern: string;
  path_pattern: string;
  method: string;
  enabled: boolean;
}

function matcherToForm(m: {
  host_pattern: string | null;
  path_pattern: string | null;
  method: string | null;
  enabled: boolean;
}): MatcherForm {
  return {
    host_pattern: m.host_pattern ?? "",
    path_pattern: m.path_pattern ?? "",
    method: m.method ?? "",
    enabled: m.enabled,
  };
}

function formToMatcher(f: MatcherForm) {
  return {
    host_pattern: f.host_pattern || null,
    path_pattern: f.path_pattern || null,
    method: f.method.toUpperCase() || null,
    enabled: f.enabled,
  };
}

// ────────────────────────────────────────────────────────────────────
// 小输入组件
// ────────────────────────────────────────────────────────────────────

function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: React.ReactNode;
  hint?: string;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
      {hint && <span className="text-[10px] text-muted-foreground/70">{hint}</span>}
    </label>
  );
}

const inputCls =
  "rounded border border-border bg-background px-2 py-1 text-sm font-mono";

// ────────────────────────────────────────────────────────────────────
// 主组件
// ────────────────────────────────────────────────────────────────────

export function RuleEditorModal({
  open,
  onClose,
  onSaved,
  editing, // 编辑已有规则时传入
}: {
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
  editing?: RuleEntry | null;
}) {
  const [name, setName] = useState(editing?.name ?? "");
  const [sortKey, setSortKey] = useState(editing?.sort_key ?? 100);
  const [ruleType, setRuleType] = useState<RuleType>(
    editing?.rule.rule_type ?? "breakpoint",
  );

  // matcher
  const initMatcher = editing
    ? matcherToForm((editing.rule as { match_: Parameters<typeof matcherToForm>[0] }).match_)
    : { host_pattern: "", path_pattern: "", method: "", enabled: true };
  const [matcher, setMatcher] = useState<MatcherForm>(initMatcher);

  // breakpoint
  const bpInit = editing?.rule.rule_type === "breakpoint" ? editing.rule : null;
  const [onRequest, setOnRequest] = useState(bpInit?.on_request ?? true);
  const [onResponse, setOnResponse] = useState(bpInit?.on_response ?? false);

  // map_local
  const mlInit = editing?.rule.rule_type === "map_local" ? editing.rule : null;
  const [localBody, setLocalBody] = useState(mlInit?.local_body ?? "{}");
  const [contentType, setContentType] = useState(mlInit?.content_type ?? "application/json");
  const [statusCode, setStatusCode] = useState(mlInit?.status_code ?? 200);

  // map_remote
  const mrInit = editing?.rule.rule_type === "map_remote" ? editing.rule : null;
  const [targetUrl, setTargetUrl] = useState(mrInit?.target_url ?? "");

  // rewrite
  const rwInit = editing?.rule.rule_type === "rewrite" ? editing.rule : null;
  const [operations, setOperations] = useState<RewriteOp[]>(rwInit?.operations ?? []);

  // block
  const blInit = editing?.rule.rule_type === "block" ? editing.rule : null;
  const [blockStatus, setBlockStatus] = useState(blInit?.status_code ?? 403);

  // throttle（限速未实现：值仅供展示已保存配置，输入已禁用）
  const thInit = editing?.rule.rule_type === "throttle_or_delay" ? editing.rule : null;
  const [delayMs, setDelayMs] = useState(Number(thInit?.delay_ms ?? 0));
  const [throughput] = useState(
    thInit?.throughput_kbps != null ? Number(thInit.throughput_kbps) : 0,
  );

  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 构造 InterceptRule
  const buildRule = (): InterceptRule | null => {
    const m = formToMatcher(matcher);
    switch (ruleType) {
      case "breakpoint":
        return { rule_type: "breakpoint", match_: m, on_request: onRequest, on_response: onResponse };
      case "map_local":
        return {
          rule_type: "map_local",
          match_: m,
          local_body: localBody,
          content_type: contentType || null,
          status_code: statusCode || null,
        };
      case "map_remote":
        if (!targetUrl) {
          setError("Map Remote 需要填写目标 URL 前缀");
          return null;
        }
        return { rule_type: "map_remote", match_: m, target_url: targetUrl };
      case "rewrite":
        if (operations.length === 0) {
          setError("Rewrite 规则至少需要一个操作");
          return null;
        }
        return { rule_type: "rewrite", match_: m, operations };
      case "block":
        return { rule_type: "block", match_: m, status_code: blockStatus || null };
      case "throttle_or_delay":
        return {
          rule_type: "throttle_or_delay",
          match_: m,
          delay_ms: delayMs || 0,
          throughput_kbps: throughput || null,
        };
    }
  };

  const handleSave = async () => {
    setError(null);
    const rule = buildRule();
    if (!rule) return;

    if (!name.trim()) {
      setError("请填写规则名称");
      return;
    }

    setSaving(true);
    try {
      if (editing) {
        await updateRule(editing.id, {
          name: name.trim(),
          rule,
          sort_key: sortKey,
        });
      } else {
        await createRule({
          name: name.trim(),
          rule,
          sort_key: sortKey,
        });
      }
      onSaved();
      onClose();
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
      width="max-w-3xl"
      title={editing ? `编辑规则：${editing.name}` : "新建拦截规则"}
      footer={
        <>
          {error && (
            <span className="mr-auto text-xs text-red-600 dark:text-red-400">{error}</span>
          )}
          <button
            onClick={onClose}
            className="rounded border border-border px-3 py-1.5 text-sm hover:bg-accent"
          >
            取消
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="rounded bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {/* 基本信息 */}
        <div className="grid grid-cols-[1fr_8rem] gap-3">
          <Field label="规则名称">
            <input
              className={inputCls}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="如：给 api.example.com 加 X-Custom header"
            />
          </Field>
          <Field label="排序（越小越先匹配）">
            <input
              type="number"
              className={inputCls}
              value={sortKey}
              onChange={(e) => setSortKey(Number(e.target.value) || 100)}
            />
          </Field>
        </div>

        {/* 规则类型 */}
        <Field label="规则类型">
          <div className="flex flex-wrap gap-1">
            {(Object.keys(RULE_TYPE_LABELS) as RuleType[]).map((t) => (
              <button
                key={t}
                onClick={() => setRuleType(t)}
                className={cn(
                  "rounded border px-2.5 py-1 text-xs transition-colors",
                  ruleType === t
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-border hover:bg-accent",
                )}
              >
                {RULE_TYPE_LABELS[t]}
              </button>
            ))}
          </div>
        </Field>

        {/* Matcher */}
        <div className="rounded border border-border/60 p-3">
          <div className="mb-2 flex items-center justify-between">
            <h4 className="text-xs font-semibold">匹配条件</h4>
            <label className="flex items-center gap-1.5 text-xs">
              <input
                type="checkbox"
                checked={matcher.enabled}
                onChange={(e) => setMatcher({ ...matcher, enabled: e.target.checked })}
              />
              启用
            </label>
          </div>
          <div className="grid grid-cols-3 gap-3">
            <Field label="Host（glob）" hint="如 *.example.com">
              <input
                className={inputCls}
                value={matcher.host_pattern}
                onChange={(e) => setMatcher({ ...matcher, host_pattern: e.target.value })}
                placeholder="*.example.com"
              />
            </Field>
            <Field label="Path（glob）" hint="如 /api/v1/*">
              <input
                className={inputCls}
                value={matcher.path_pattern}
                onChange={(e) => setMatcher({ ...matcher, path_pattern: e.target.value })}
                placeholder="/api/*"
              />
            </Field>
            <Field label="Method" hint="留空匹配全部">
              <input
                className={inputCls}
                value={matcher.method}
                onChange={(e) => setMatcher({ ...matcher, method: e.target.value })}
                placeholder="GET"
              />
            </Field>
          </div>
        </div>

        {/* 类型特有配置 */}
        <div className="rounded border border-border/60 p-3">
          <h4 className="mb-2 text-xs font-semibold">规则配置</h4>

          {ruleType === "breakpoint" && (
            <div className="flex gap-4 text-sm">
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={onRequest}
                  onChange={(e) => setOnRequest(e.target.checked)}
                />
                请求阶段断点
              </label>
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={onResponse}
                  onChange={(e) => setOnResponse(e.target.checked)}
                />
                响应阶段断点
              </label>
            </div>
          )}

          {ruleType === "map_local" && (
            <div className="flex flex-col gap-3">
              <div className="grid grid-cols-[8rem_6rem_1fr] gap-3">
                <Field label="状态码">
                  <input
                    type="number"
                    className={inputCls}
                    value={statusCode}
                    onChange={(e) => setStatusCode(Number(e.target.value) || 200)}
                  />
                </Field>
                <Field label="Content-Type">
                  <input
                    className={inputCls}
                    value={contentType}
                    onChange={(e) => setContentType(e.target.value)}
                  />
                </Field>
              </div>
              <Field label="本地响应 Body">
                <textarea
                  className={cn(inputCls, "min-h-24 font-mono")}
                  value={localBody}
                  onChange={(e) => setLocalBody(e.target.value)}
                />
              </Field>
            </div>
          )}

          {ruleType === "map_remote" && (
            <Field label="目标 URL 前缀" hint="原请求 URL 的 host 部分被替换为此前缀，path 保留">
              <input
                className={inputCls}
                value={targetUrl}
                onChange={(e) => setTargetUrl(e.target.value)}
                placeholder="https://staging.example.com"
              />
            </Field>
          )}

          {ruleType === "rewrite" && (
            <RewriteOpsEditor
              operations={operations}
              onChange={setOperations}
            />
          )}

          {ruleType === "block" && (
            <Field label="响应状态码" hint="默认 403">
              <input
                type="number"
                className={cn(inputCls, "w-24")}
                value={blockStatus}
                onChange={(e) => setBlockStatus(Number(e.target.value) || 403)}
              />
            </Field>
          )}

          {ruleType === "throttle_or_delay" && (
            <div className="grid grid-cols-2 gap-3">
              <Field label="延迟（毫秒）" hint="转发前总延迟，多条规则叠加时求和">
                <input
                  type="number"
                  className={inputCls}
                  value={delayMs}
                  min={0}
                  onChange={(e) => setDelayMs(Math.max(0, Number(e.target.value) || 0))}
                />
              </Field>
              <Field label="限速（KB/s）" hint="尚未实现，保存后不生效">
                <input
                  type="number"
                  className={cn(inputCls, "cursor-not-allowed opacity-50")}
                  value={throughput}
                  min={0}
                  disabled
                  title="限速功能尚未实现，当前仅支持延迟"
                />
              </Field>
            </div>
          )}
        </div>

        {/* Rewrite diff 预览 */}
        {ruleType === "rewrite" && operations.length > 0 && (
          <div className="rounded border border-border/60 p-3">
            <h4 className="mb-2 text-xs font-semibold">Diff 预览</h4>
            <RewriteDiffView original={SAMPLE_MESSAGE} operations={operations} />
          </div>
        )}
      </div>
    </Modal>
  );
}

// ────────────────────────────────────────────────────────────────────
// Rewrite 操作列表编辑器
// ────────────────────────────────────────────────────────────────────

function RewriteOpsEditor({
  operations,
  onChange,
}: {
  operations: RewriteOp[];
  onChange: (ops: RewriteOp[]) => void;
}) {
  const update = (i: number, op: RewriteOp) => {
    const next = [...operations];
    next[i] = op;
    onChange(next);
  };

  return (
    <div className="flex flex-col gap-2">
      {operations.length === 0 && (
        <p className="text-xs text-muted-foreground">
          尚未添加操作。改写操作按顺序依次应用。
        </p>
      )}

      {operations.map((op, i) => (
        <div key={i} className="flex items-start gap-2 rounded bg-muted/30 p-2">
          <span className="mt-1.5 w-5 text-center text-xs text-muted-foreground">{i + 1}</span>

          <select
            className={cn(inputCls, "w-36")}
            value={op.op}
            onChange={(e) => {
              const kind = e.target.value as RewriteOp["op"];
              const defaults: Record<RewriteOp["op"], RewriteOp> = {
                set_header: { op: "set_header", name: "X-Custom", value: "" },
                remove_header: { op: "remove_header", name: "" },
                replace_body: { op: "replace_body", pattern: "", replacement: "" },
                set_body: { op: "set_body", content: "" },
              };
              update(i, defaults[kind]);
            }}
          >
            <option value="set_header">设置 Header</option>
            <option value="remove_header">删除 Header</option>
            <option value="replace_body">正则替换 Body</option>
            <option value="set_body">替换整个 Body</option>
          </select>

          {/* 各类型的参数输入 */}
          <div className="flex flex-1 flex-wrap gap-2">
            {op.op === "set_header" && (
              <>
                <input
                  className={cn(inputCls, "w-40")}
                  placeholder="Header 名"
                  value={op.name}
                  onChange={(e) => update(i, { ...op, name: e.target.value })}
                />
                <input
                  className={cn(inputCls, "flex-1")}
                  placeholder="值"
                  value={op.value}
                  onChange={(e) => update(i, { ...op, value: e.target.value })}
                />
              </>
            )}
            {op.op === "remove_header" && (
              <input
                className={cn(inputCls, "w-56")}
                placeholder="Header 名（大小写不敏感）"
                value={op.name}
                onChange={(e) => update(i, { ...op, name: e.target.value })}
              />
            )}
            {op.op === "replace_body" && (
              <>
                <input
                  className={cn(inputCls, "flex-1")}
                  placeholder="正则（如 'prod'）"
                  value={op.pattern}
                  onChange={(e) => update(i, { ...op, pattern: e.target.value })}
                />
                <input
                  className={cn(inputCls, "flex-1")}
                  placeholder="替换为（支持 $1 反向引用）"
                  value={op.replacement}
                  onChange={(e) => update(i, { ...op, replacement: e.target.value })}
                />
              </>
            )}
            {op.op === "set_body" && (
              <textarea
                className={cn(inputCls, "min-h-16 flex-1")}
                placeholder="新的 body 内容"
                value={op.content}
                onChange={(e) => update(i, { ...op, content: e.target.value })}
              />
            )}
          </div>

          <button
            onClick={() => onChange(operations.filter((_, j) => j !== i))}
            className="mt-1 rounded p-1 text-muted-foreground hover:bg-accent hover:text-red-500"
            title="删除此操作"
          >
            ✕
          </button>
        </div>
      ))}

      <button
        onClick={() =>
          onChange([...operations, { op: "set_header", name: "X-Custom", value: "" }])
        }
        className="self-start rounded border border-dashed border-border px-3 py-1 text-xs text-muted-foreground hover:bg-accent"
      >
        + 添加操作
      </button>
    </div>
  );
}
