/**
 * 全局 Jotai atoms（`spec.md` 6.1 节、6.2 节）。
 *
 * M1 阶段管理：
 * - 当前选中的 Workspace ID
 * - 当前选中的请求 ID（Collection 树中点击的请求）
 * - 当前选中的 Environment ID（环境切换下拉框）
 * - App 模式（Client / Proxy）
 */

import { atom } from "jotai";

export type AppMode = "client" | "proxy";

/** App 模式：Client（API 客户端）或 Proxy（抓包） */
export const appModeAtom = atom<AppMode>("client");

/** 当前选中的 Workspace ID */
export const currentWorkspaceIdAtom = atom<string | null>(null);

/** 当前选中的请求 ID（Collection 树中点击的请求节点） */
export const selectedRequestIdAtom = atom<string | null>(null);

/** 当前选中的 Environment ID（环境切换下拉框） */
export const currentEnvironmentIdAtom = atom<string | null>(null);
