/**
 * Rewrite 规则 diff 预览（`plan.md` M3.2：基于并排视图展示修改前后对比）。
 *
 * 输入一个示例消息（原始 headers/body）+ Rewrite 操作列表，
 * 在前端模拟规则引擎的改写逻辑，并排展示 Before / After。
 *
 * 改写逻辑与 `cuckoo-proxy/src/rule_engine.rs` 的 `apply_rewrite_op` 保持一致：
 * - SetHeader：覆盖同名 header（移除后追加）
 * - RemoveHeader：移除所有同名 header（大小写不敏感）
 * - ReplaceBody：正则替换 body 文本
 * - SetBody：整体替换 body
 */

import { useMemo } from "react";
import type { ProxyHttpMessage, RewriteOp } from "@/lib/api/generated";

/** 应用 Rewrite 操作列表到消息（与后端 apply_rewrite_op 逻辑一致） */
function applyOps(original: ProxyHttpMessage, ops: RewriteOp[]): ProxyHttpMessage {
  let msg: ProxyHttpMessage = { ...original, headers: [...original.headers] };

  for (const op of ops) {
    switch (op.op) {
      case "set_header":
        msg = setHeaderExact(msg, op.name, op.value);
        break;
      case "remove_header":
        msg = {
          ...msg,
          headers: msg.headers.filter(
            ([k]) => k.toLowerCase() !== op.name.toLowerCase(),
          ),
        };
        break;
      case "replace_body": {
        try {
          const text = new TextDecoder().decode(new Uint8Array(msg.body));
          const re = new RegExp(op.pattern, "g");
          const replaced = text.replace(re, op.replacement.replace(/\$(\d)/g, "$$$1"));
          const bytes = Array.from(new TextEncoder().encode(replaced));
          msg = setHeaderExact({ ...msg, body: bytes }, "Content-Length", String(bytes.length));
        } catch {
          // 无效正则，保持原样
        }
        break;
      }
      case "set_body": {
        const bytes = Array.from(new TextEncoder().encode(op.content));
        msg = setHeaderExact({ ...msg, body: bytes }, "Content-Length", String(bytes.length));
        break;
      }
    }
  }
  return msg;
}

function setHeaderExact(msg: ProxyHttpMessage, name: string, value: string): ProxyHttpMessage {
  const filtered = msg.headers.filter(
    ([k]) => k.toLowerCase() !== name.toLowerCase(),
  );
  return { ...msg, headers: [...filtered, [name, value]] };
}

/** diff 计算：找出新增/删除/变化的行 */
function diffHeaders(
  before: [string, string][],
  after: [string, string][],
): { line: string; kind: "same" | "add" | "del" }[] {
  const result: { line: string; kind: "same" | "add" | "del" }[] = [];
  const afterUsed = new Set<number>();

  for (const [k, v] of before) {
    const idx = after.findIndex(
      ([k2, v2], i) => !afterUsed.has(i) && k2 === k && v2 === v,
    );
    if (idx >= 0) {
      afterUsed.add(idx);
      result.push({ line: `${k}: ${v}`, kind: "same" });
    } else {
      const changed = after.findIndex(
        ([k2], i) => !afterUsed.has(i) && k2.toLowerCase() === k.toLowerCase(),
      );
      if (changed >= 0) {
        afterUsed.add(changed);
        result.push({ line: `${k}: ${v}`, kind: "del" });
        result.push({ line: `${after[changed][0]}: ${after[changed][1]}`, kind: "add" });
      } else {
        result.push({ line: `${k}: ${v}`, kind: "del" });
      }
    }
  }

  after.forEach(([k, v], i) => {
    if (!afterUsed.has(i)) {
      result.push({ line: `${k}: ${v}`, kind: "add" });
    }
  });

  return result;
}

function bodyText(body: number[]): string {
  if (body.length === 0) return "(empty body)";
  try {
    return new TextDecoder().decode(new Uint8Array(body));
  } catch {
    return `(binary data, ${body.length} bytes)`;
  }
}

export function RewriteDiffView({
  original,
  operations,
}: {
  original: ProxyHttpMessage;
  operations: RewriteOp[];
}) {
  const after = useMemo(() => applyOps(original, operations), [original, operations]);
  const headerDiff = useMemo(
    () => diffHeaders(original.headers, after.headers),
    [original, after],
  );

  const beforeBody = bodyText(original.body);
  const afterBody = bodyText(after.body);
  const bodyChanged = beforeBody !== afterBody;

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted-foreground">
        预览：左侧为原始消息，右侧为应用 {operations.length} 个改写操作后的结果
      </p>

      {/* Headers diff */}
      <div>
        <h4 className="mb-1 text-xs font-medium">Headers（红=删除，绿=新增/修改）</h4>
        <div className="grid grid-cols-2 gap-2">
          <div>
            <div className="mb-1 text-[10px] uppercase text-muted-foreground">Before</div>
            <pre className="max-h-48 overflow-auto rounded bg-muted/50 p-2 font-mono text-xs">
              {headerDiff
                .filter((d) => d.kind !== "add")
                .map((d) => d.line)
                .join("\n") || "(no headers)"}
            </pre>
          </div>
          <div>
            <div className="mb-1 text-[10px] uppercase text-muted-foreground">After</div>
            <pre className="max-h-48 overflow-auto rounded bg-muted/50 p-2 font-mono text-xs">
              {headerDiff
                .filter((d) => d.kind !== "del")
                .map((d) => d.line)
                .join("\n") || "(no headers)"}
            </pre>
          </div>
        </div>
      </div>

      {/* Body diff */}
      <div>
        <h4 className="mb-1 text-xs font-medium">
          Body
          {bodyChanged && (
            <span className="ml-2 rounded bg-green-100 px-1.5 py-0.5 text-[10px] text-green-700 dark:bg-green-900/30 dark:text-green-400">
              已修改
            </span>
          )}
        </h4>
        <div className="grid grid-cols-2 gap-2">
          <pre className="max-h-48 overflow-auto rounded bg-muted/50 p-2 font-mono text-xs">
            {beforeBody}
          </pre>
          <pre
            className={
              bodyChanged
                ? "max-h-48 overflow-auto rounded bg-green-50 p-2 font-mono text-xs dark:bg-green-950/30"
                : "max-h-48 overflow-auto rounded bg-muted/50 p-2 font-mono text-xs"
            }
          >
            {afterBody}
          </pre>
        </div>
      </div>
    </div>
  );
}
