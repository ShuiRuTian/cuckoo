/**
 * 通用模态框组件（原生 Tailwind 实现）。
 *
 * - 点击遮罩 / Esc 关闭（可通过 disabled 关闭点击遮罩关闭）
 * - 标题栏 + 内容区 + 底部操作区
 */

import { useEffect, type ReactNode } from "react";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

export function Modal({
  open,
  title,
  onClose,
  children,
  footer,
  width = "max-w-2xl",
  closeOnOverlay = true,
}: {
  open: boolean;
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: string;
  closeOnOverlay?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* 遮罩 */}
      <div
        className="absolute inset-0 bg-black/50"
        onClick={closeOnOverlay ? onClose : undefined}
      />

      {/* 模态框主体 */}
      <div
        className={cn(
          "relative flex max-h-[85vh] w-full flex-col rounded-lg border border-border bg-card shadow-xl",
          width,
        )}
        role="dialog"
        aria-modal="true"
      >
        {/* 标题栏 */}
        <div className="flex items-center justify-between border-b px-4 py-2">
          <h2 className="text-sm font-semibold">{title}</h2>
          <button
            onClick={onClose}
            className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 内容区 */}
        <div className="flex-1 overflow-auto p-4">{children}</div>

        {/* 底部操作区 */}
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t px-4 py-2">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}
