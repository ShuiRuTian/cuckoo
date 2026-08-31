/**
 * 响应查看器（`spec.md` 6.2 节、`plan.md` M1.4）。
 *
 * M1 阶段实现：
 * - 状态码 + 状态文本 + 耗时
 * - Response Headers 表格
 * - Response Body（JSON 自动美化）
 *
 * 后续补齐（M5）：Preview/Raw/Cookies/Timing 瀑布图 Tab
 */

import { useState } from "react";
import { cn } from "@/lib/utils";
import type { ExecutionResult } from "@/lib/api/generated";

type ResponseTab = "body" | "headers";

export function ResponseViewer({ result }: { result: ExecutionResult }) {
  const [tab, setTab] = useState<ResponseTab>("body");

  const statusColor = result.success
    ? "text-green-600 dark:text-green-400"
    : "text-red-600 dark:text-red-400";

  // 尝试美化 JSON
  let prettyBody = result.body;
  try {
    const parsed = JSON.parse(result.body);
    prettyBody = JSON.stringify(parsed, null, 2);
  } catch {
    // 非 JSON，保持原文
  }

  return (
    <div className="flex h-full flex-col">
      {/* 状态栏 */}
      <div className="flex items-center gap-3 border-b px-3 py-1.5 text-xs">
        <span className={cn("font-bold", statusColor)}>
          {result.status} {result.status_text}
        </span>
        <span className="text-muted-foreground">
          {result.total_time_ms}ms
        </span>
        <span className="text-muted-foreground">
          {result.body_size} bytes
        </span>
        {result.content_type && (
          <span className="text-muted-foreground">
            {result.content_type}
          </span>
        )}
        {result.error && (
          <span className="text-red-600 dark:text-red-400">
            {result.error}
          </span>
        )}
      </div>

      {/* Tab 栏 */}
      <div className="flex border-b">
        {(
          [
            ["body", "Body"],
            ["headers", "Headers"],
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
        {tab === "body" && (
          <pre className="h-full overflow-auto rounded bg-muted/50 p-2 font-mono text-xs">
            {prettyBody || "(empty body)"}
          </pre>
        )}
        {tab === "headers" && (
          <table className="w-full text-xs">
            <thead>
              <tr className="border-b">
                <th className="px-2 py-1 text-left font-medium">Name</th>
                <th className="px-2 py-1 text-left font-medium">Value</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(result.headers).map(([name, value]) => (
                <tr key={name} className="border-b border-border/50">
                  <td className="px-2 py-1 font-mono">{name}</td>
                  <td className="px-2 py-1 font-mono break-all">{value}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
