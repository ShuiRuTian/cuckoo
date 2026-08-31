import {
  useCallback,
  useRef,
  useState,
  type ReactNode,
  type PointerEvent,
} from "react";
import { cn } from "@/lib/utils";

/**
 * 自研可调整大小面板组件（`spec.md` 6.1 节、`plan.md` M0）。
 *
 * M0 最简版本：支持水平/垂直切分、拖拽 divider 调整比例。
 * 嵌套（在子面板里再放一个 `ResizablePanel`）/ 持久化 / 键盘可访问性可以后续迭代补齐。
 *
 * 用法：
 * ```tsx
 * <ResizablePanel direction="horizontal">
 *   <div>左侧</div>
 *   <div>右侧</div>
 * </ResizablePanel>
 * ```
 */
export interface ResizablePanelProps {
  /** 切分方向：`horizontal` = 左右分栏，`vertical` = 上下分栏。 */
  direction: "horizontal" | "vertical";
  /** 初始比例（第一个面板占比），默认 0.5。 */
  initialRatio?: number;
  /** 最小比例（防止面板被拖到看不见），默认 0.1。 */
  minRatio?: number;
  /** 最大比例，默认 0.9。 */
  maxRatio?: number;
  /** 分割条粗细（px），默认 6。 */
  dividerSize?: number;
  /** className 透传到外层容器。 */
  className?: string;
  /** 两个子面板内容。 */
  children: [ReactNode, ReactNode];
}

export function ResizablePanel({
  direction,
  initialRatio = 0.5,
  minRatio = 0.1,
  maxRatio = 0.9,
  dividerSize = 6,
  className,
  children,
}: ResizablePanelProps) {
  const [ratio, setRatio] = useState(initialRatio);
  const containerRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef(false);

  const onPointerDown = useCallback((e: PointerEvent) => {
    e.preventDefault();
    draggingRef.current = true;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
      if (!draggingRef.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      let newRatio: number;
      if (direction === "horizontal") {
        newRatio = (e.clientX - rect.left) / rect.width;
      } else {
        newRatio = (e.clientY - rect.top) / rect.height;
      }
      newRatio = Math.max(minRatio, Math.min(maxRatio, newRatio));
      setRatio(newRatio);
    },
    [direction, minRatio, maxRatio],
  );

  const onPointerUp = useCallback((e: PointerEvent) => {
    draggingRef.current = false;
    try {
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      // ignore
    }
  }, []);

  const isHorizontal = direction === "horizontal";

  return (
    <div
      ref={containerRef}
      className={cn("flex", isHorizontal ? "flex-row" : "flex-col", className)}
    >
      <div
        style={{
          flexBasis: `${ratio * 100}%`,
          flexGrow: 0,
          flexShrink: 0,
          overflow: "hidden",
        }}
      >
        {children[0]}
      </div>
      <div
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        style={{
          flexBasis: dividerSize,
          flexGrow: 0,
          flexShrink: 0,
          cursor: isHorizontal ? "col-resize" : "row-resize",
        }}
        className={cn(
          "bg-border hover:bg-primary/20 transition-colors select-none touch-none",
        )}
      />
      <div style={{ flexGrow: 1, overflow: "hidden" }}>{children[1]}</div>
    </div>
  );
}
