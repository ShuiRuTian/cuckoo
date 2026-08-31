/**
 * KeyValueEditor 通用组件（`spec.md` 6.2 节）。
 *
 * 用于编辑 Headers / Query Params / Env Variables 等 Key-Value 列表。
 * 每行包含：enabled checkbox + key input + value input + delete button。
 * 底部有 "Add" 按钮添加新行。
 *
 * 泛型设计：兼容 HeaderEntry（name 字段）和 KeyValueEntry/EnvVariable（key 字段）等不同类型。
 */

import { Plus, Trash2, Check } from "lucide-react";
import { cn } from "@/lib/utils";

/** 统一的编辑器内部条目格式 */
export interface KVEntry {
  key: string;
  value: string;
  enabled: boolean;
}

export interface KeyValueEditorProps<T> {
  /** 原始数据列表 */
  entries: T[];
  /** 从原始数据转换为 KVEntry */
  toEntries: (items: T[]) => KVEntry[];
  /** 从 KVEntry 转换回原始数据 */
  fromEntries: (entries: KVEntry[]) => T[];
  /** 数据变更回调 */
  onChange: (items: T[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
}

export function KeyValueEditor<T>({
  entries,
  toEntries,
  fromEntries,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
}: KeyValueEditorProps<T>) {
  const kvEntries = toEntries(entries);

  const handleAdd = () => {
    const newEntries = [...kvEntries, { key: "", value: "", enabled: true }];
    onChange(fromEntries(newEntries));
  };

  const handleRemove = (index: number) => {
    const newEntries = kvEntries.filter((_, i) => i !== index);
    onChange(fromEntries(newEntries));
  };

  const handleChange = (index: number, field: keyof KVEntry, value: string | boolean) => {
    const newEntries = kvEntries.map((e, i) =>
      i === index ? { ...e, [field]: value } : e,
    );
    onChange(fromEntries(newEntries));
  };

  return (
    <div className="flex flex-col gap-1">
      {kvEntries.length === 0 && (
        <p className="py-2 text-xs text-muted-foreground">暂无条目</p>
      )}
      {kvEntries.map((entry, i) => (
        <div key={i} className="flex items-center gap-1">
          <button
            onClick={() => handleChange(i, "enabled", !entry.enabled)}
            className={cn(
              "flex h-5 w-5 shrink-0 items-center justify-center rounded border",
              entry.enabled
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border bg-background",
            )}
          >
            {entry.enabled && <Check className="h-3 w-3" />}
          </button>
          <input
            type="text"
            value={entry.key}
            onChange={(e) => handleChange(i, "key", e.target.value)}
            placeholder={keyPlaceholder}
            className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
          />
          <input
            type="text"
            value={entry.value}
            onChange={(e) => handleChange(i, "value", e.target.value)}
            placeholder={valuePlaceholder}
            className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
          />
          <button
            onClick={() => handleRemove(i)}
            className="rounded p-1 hover:bg-accent"
          >
            <Trash2 className="h-3 w-3 text-muted-foreground" />
          </button>
        </div>
      ))}
      <button
        onClick={handleAdd}
        className="flex w-fit items-center gap-1 rounded border border-border px-2 py-1 text-xs hover:bg-accent"
      >
        <Plus className="h-3 w-3" />
        Add
      </button>
    </div>
  );
}
