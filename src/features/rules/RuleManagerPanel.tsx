/**
 * 拦截规则列表管理界面（`plan.md` M3.2）。
 *
 * 功能：
 * - 规则列表（名称 / 类型徽章 / 匹配条件摘要 / 启用开关 / 排序 / 删除）
 * - 新建规则（打开 RuleEditorModal）
 * - 编辑规则
 * - 清空全部规则
 * - 显示当前挂起中的断点（可点击唤起编辑模态）
 *
 * 数据流：直接调用生成的 REST 客户端（listRules/updateRule/deleteRule/clearRules）。
 */

import { useCallback, useEffect, useState } from "react";
import { Plus, Trash2, Pencil, Ban, AlertCircle } from "lucide-react";
import { RuleEditorModal } from "./RuleEditorModal";
import {
  listRules,
  updateRule,
  deleteRule,
  clearRules,
  listPendingIntercepts,
  type InterceptRule,
  type RuleEntry,
} from "@/lib/api/generated";
import { cn } from "@/lib/utils";

/** 规则类型徽章样式 */
const TYPE_BADGES: Record<InterceptRule["rule_type"], { label: string; cls: string }> = {
  breakpoint: { label: "断点", cls: "bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400" },
  map_local: { label: "Map Local", cls: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400" },
  map_remote: { label: "Map Remote", cls: "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400" },
  rewrite: { label: "Rewrite", cls: "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400" },
  block: { label: "Block", cls: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400" },
  throttle_or_delay: { label: "延迟", cls: "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300" },
};

/** 匹配条件摘要 */
function matcherSummary(entry: RuleEntry): string {
  const m = entry.rule.match_;
  const parts: string[] = [];
  if (m.method) parts.push(m.method);
  if (m.host_pattern) parts.push(m.host_pattern);
  if (m.path_pattern) parts.push(m.path_pattern);
  return parts.length > 0 ? parts.join("  ") : "匹配全部请求";
}

export function RuleManagerPanel({
  onOpenIntercept,
}: {
  /** 点击挂起断点时回调（打开断点编辑模态） */
  onOpenIntercept: (flowId: string, stage: string) => void;
}) {
  const [rules, setRules] = useState<RuleEntry[]>([]);
  const [pending, setPending] = useState<{ flow_id: string; stage: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editing, setEditing] = useState<RuleEntry | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const [r, p] = await Promise.all([listRules(), listPendingIntercepts()]);
      setRules(r);
      setPending(p);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 挂起断点轮询（10s，兜底 SSE 事件丢失的场景）
  useEffect(() => {
    const timer = setInterval(refresh, 10_000);
    return () => clearInterval(timer);
  }, [refresh]);

  const handleToggleEnabled = async (entry: RuleEntry, enabled: boolean) => {
    try {
      // 保留原 matcher 全部字段，仅切换 enabled
      const updated = await updateRule(entry.id, {
        name: entry.name,
        sort_key: entry.sort_key,
        rule: {
          ...entry.rule,
          match_: { ...entry.rule.match_, enabled },
        },
      });
      setRules((prev) => prev.map((r) => (r.id === updated.id ? updated : r)));
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (entry: RuleEntry) => {
    if (!window.confirm(`确定删除规则「${entry.name}」？`)) return;
    try {
      await deleteRule(entry.id);
      setRules((prev) => prev.filter((r) => r.id !== entry.id));
    } catch (e) {
      setError(String(e));
    }
  };

  const handleClearAll = async () => {
    if (rules.length === 0) return;
    if (!window.confirm(`确定清空全部 ${rules.length} 条规则？`)) return;
    try {
      await clearRules();
      setRules([]);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <button
          onClick={() => {
            setEditing(null);
            setEditorOpen(true);
          }}
          className="flex items-center gap-1 rounded bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
        >
          <Plus className="h-3.5 w-3.5" />
          新建规则
        </button>
        <span className="text-xs text-muted-foreground">
          {rules.length} 条规则，按排序值升序匹配
        </span>
        <button
          onClick={handleClearAll}
          disabled={rules.length === 0}
          className="ml-auto flex items-center gap-1 rounded border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent disabled:opacity-40"
        >
          <Ban className="h-3 w-3" />
          清空全部
        </button>
      </div>

      {error && (
        <div className="border-b bg-red-100 px-3 py-1.5 text-xs text-red-700 dark:bg-red-900/30 dark:text-red-400">
          {error}
        </div>
      )}

      {/* 挂起断点提示区 */}
      {pending.length > 0 && (
        <div className="border-b bg-yellow-50 px-3 py-2 dark:bg-yellow-950/30">
          <div className="mb-1 flex items-center gap-1.5 text-xs font-medium text-yellow-700 dark:text-yellow-400">
            <AlertCircle className="h-3.5 w-3.5" />
            {pending.length} 个断点挂起中
          </div>
          <div className="flex flex-wrap gap-1.5">
            {pending.map((p) => (
              <button
                key={p.flow_id}
                onClick={() => onOpenIntercept(p.flow_id, p.stage)}
                className="rounded border border-yellow-300 bg-white px-2 py-0.5 font-mono text-[11px] hover:bg-yellow-100 dark:border-yellow-700 dark:bg-card dark:hover:bg-yellow-900/40"
              >
                {p.stage === "request" ? "请求" : "响应"} · {p.flow_id.slice(0, 10)}…
              </button>
            ))}
          </div>
        </div>
      )}

      {/* 规则列表 */}
      <div className="flex-1 overflow-auto">
        {loading && (
          <p className="p-3 text-xs text-muted-foreground">加载中...</p>
        )}

        {!loading && rules.length === 0 && (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-muted-foreground">
            <p className="text-sm">暂无拦截规则</p>
            <p className="text-xs">
              新建一条 Breakpoint 规则体验断点编辑，或用 Rewrite 给请求加自定义 header
            </p>
          </div>
        )}

        <table className="w-full text-xs">
          <thead>
            <tr className="border-b bg-muted/40">
              <th className="w-16 px-2 py-1.5 text-left font-medium">排序</th>
              <th className="px-2 py-1.5 text-left font-medium">类型</th>
              <th className="px-2 py-1.5 text-left font-medium">名称</th>
              <th className="px-2 py-1.5 text-left font-medium">匹配条件</th>
              <th className="w-20 px-2 py-1.5 text-center font-medium">启用</th>
              <th className="w-24 px-2 py-1.5 text-right font-medium">操作</th>
            </tr>
          </thead>
          <tbody>
            {rules.map((entry) => {
              const badge = TYPE_BADGES[entry.rule.rule_type];
              const enabled = entry.rule.match_.enabled;
              return (
                <tr
                  key={entry.id}
                  className={cn(
                    "border-b border-border/30 hover:bg-accent/30",
                    !enabled && "opacity-50",
                  )}
                >
                  <td className="px-2 py-1.5 font-mono">{entry.sort_key}</td>
                  <td className="px-2 py-1.5">
                    <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium", badge.cls)}>
                      {badge.label}
                    </span>
                  </td>
                  <td className="px-2 py-1.5 font-medium">{entry.name}</td>
                  <td className="px-2 py-1.5 font-mono text-muted-foreground">
                    {matcherSummary(entry)}
                  </td>
                  <td className="px-2 py-1.5 text-center">
                    <input
                      type="checkbox"
                      checked={enabled}
                      onChange={(e) => handleToggleEnabled(entry, e.target.checked)}
                    />
                  </td>
                  <td className="px-2 py-1.5">
                    <div className="flex justify-end gap-1">
                      <button
                        onClick={() => {
                          setEditing(entry);
                          setEditorOpen(true);
                        }}
                        className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                        title="编辑"
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </button>
                      <button
                        onClick={() => handleDelete(entry)}
                        className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-red-500"
                        title="删除"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {/* 新建/编辑模态（key 强制重建以重置表单状态） */}
      {editorOpen && (
        <RuleEditorModal
          key={editing?.id ?? "new"}
          open={editorOpen}
          editing={editing}
          onClose={() => setEditorOpen(false)}
          onSaved={refresh}
        />
      )}
    </div>
  );
}
