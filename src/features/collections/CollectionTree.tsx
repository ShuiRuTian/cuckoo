/**
 * Collection 树组件（`spec.md` 6.2 节、`plan.md` M1.4）。
 *
 * 基于 react-arborist 实现的虚拟化树形组件，展示 Workspace 下的 Folder/Request 层级结构。
 * M1 阶段支持：新建/删除 Folder 和 Request，暂不做拖拽排序（M5 补齐）。
 *
 * 数据流：
 * - TanStack Query 拉取 folders + requests → 合并为树形结构 → react-arborist 渲染
 * - 选中 Request 节点时，更新 `selectedRequestIdAtom`，触发请求编辑器加载
 */

import { useMemo, type MouseEvent as ReactMouseEvent } from "react";
import { Tree, type NodeRendererProps } from "react-arborist";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Folder as FolderIcon,
  FileText,
  ChevronRight,
  Plus,
  FolderPlus,
  Trash2,
} from "lucide-react";
import { useAtom } from "jotai";
import {
  listFolders,
  listRequests,
  createFolder,
  createRequest,
  deleteFolder,
  deleteRequest,
  type FolderModel,
  type HttpRequestDefModel,
  type CreateFolderInput,
  type CreateRequestInput,
} from "@/lib/api/generated";
import { currentWorkspaceIdAtom, selectedRequestIdAtom } from "@/state/app";
import { cn } from "@/lib/utils";

/** 树节点数据结构 */
interface TreeNode {
  id: string;
  name: string;
  /** "folder" | "request" | "workspace" */
  kind: "folder" | "request" | "workspace";
  /** 子节点（仅 folder/workspace 有） */
  children?: TreeNode[];
  /** 原始数据（用于编辑等操作） */
  raw?: FolderModel | HttpRequestDefModel;
}

/** react-arborist 需要的 id 字段 */
interface FlatNode extends TreeNode {
  parentId: string | null;
}

/** 从 folders + requests 构建树 */
function buildTree(
  folders: FolderModel[],
  requests: HttpRequestDefModel[],
): TreeNode[] {
  const folderMap = new Map<string, TreeNode>();
  const rootFolders: TreeNode[] = [];

  // 先创建所有 folder 节点
  for (const f of folders) {
    folderMap.set(f.id, {
      id: f.id,
      name: f.name,
      kind: "folder" as const,
      children: [],
      raw: f,
    });
  }

  // 构建 folder 层级
  for (const f of folders) {
    const node = folderMap.get(f.id)!;
    if (f.parent_folder_id && folderMap.has(f.parent_folder_id)) {
      folderMap.get(f.parent_folder_id)!.children!.push(node);
    } else {
      rootFolders.push(node);
    }
  }

  // 将 requests 挂到对应 folder 下（或根级别）
  for (const r of requests) {
    const reqNode: TreeNode = {
      id: r.id,
      name: r.name,
      kind: "request" as const,
      raw: r,
    };
    if (r.folder_id && folderMap.has(r.folder_id)) {
      folderMap.get(r.folder_id)!.children!.push(reqNode);
    } else {
      rootFolders.push(reqNode);
    }
  }

  return rootFolders;
}

/** 将嵌套树转为 react-arborist 需要的扁平数组 + parentId */
function flatten(nodes: TreeNode[], parentId: string | null = null): FlatNode[] {
  const result: FlatNode[] = [];
  for (const node of nodes) {
    result.push({ ...node, parentId });
    if (node.children && node.children.length > 0) {
      result.push(...flatten(node.children, node.id));
    }
  }
  return result;
}

/** 树节点渲染组件 */
function Node({ node, style, dragHandle }: NodeRendererProps<FlatNode>) {
  const [, setSelectedRequestId] = useAtom(selectedRequestIdAtom);
  const queryClient = useQueryClient();
  const data = node.data;

  const deleteMut = useMutation({
    mutationFn: (id: string) =>
      data.kind === "folder" ? deleteFolder(id) : deleteRequest(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["folders"] });
      queryClient.invalidateQueries({ queryKey: ["requests"] });
    },
  });

  const handleClick = () => {
    if (data.kind === "request") {
      setSelectedRequestId(data.id);
    }
  };

  const handleDelete = (e: ReactMouseEvent) => {
    e.stopPropagation();
    if (confirm(`确认删除 "${data.name}"？`)) {
      deleteMut.mutate(data.id);
    }
  };

  const isFolder = data.kind === "folder";
  const icon = isFolder ? (
    <FolderIcon className="h-3.5 w-3.5 text-muted-foreground" />
  ) : (
    <FileText className="h-3.5 w-3.5 text-muted-foreground" />
  );

  return (
    <div
      style={style}
      ref={dragHandle}
      className={cn(
        "group flex cursor-pointer items-center gap-1 px-2 py-0.5 text-sm",
        "hover:bg-accent/50 rounded",
        node.isSelected && "bg-accent",
        node.isSelected && "text-accent-foreground",
      )}
      onClick={handleClick}
    >
      {isFolder && (
        <ChevronRight
          className={cn(
            "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
            node.isOpen && "rotate-90",
          )}
          onClick={(e) => {
            e.stopPropagation();
            node.toggle();
          }}
        />
      )}
      {!isFolder && <span className="w-3" />}
      {icon}
      <span className="flex-1 truncate">{data.name}</span>
      <button
        onClick={handleDelete}
        className="ml-auto shrink-0 rounded p-0.5 opacity-0 hover:bg-accent group-hover:opacity-100"
        title="删除"
      >
        <Trash2 className="h-3 w-3 text-muted-foreground" />
      </button>
    </div>
  );
}

/** Collection 树主组件 */
export function CollectionTree() {
  const [workspaceId] = useAtom(currentWorkspaceIdAtom);
  const queryClient = useQueryClient();

  // 拉取 folders + requests
  const foldersQuery = useQuery({
    queryKey: ["folders", workspaceId],
    queryFn: () => listFolders(workspaceId!),
    enabled: !!workspaceId,
  });

  const requestsQuery = useQuery({
    queryKey: ["requests", workspaceId],
    queryFn: () => listRequests(workspaceId!),
    enabled: !!workspaceId,
  });

  // 构建树数据
  const treeData = useMemo(() => {
    if (!foldersQuery.data || !requestsQuery.data) return [];
    const tree = buildTree(foldersQuery.data, requestsQuery.data);
    return flatten(tree);
  }, [foldersQuery.data, requestsQuery.data]);

  // Mutations
  const createFolderMut = useMutation({
    mutationFn: (input: CreateFolderInput) => createFolder(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["folders", workspaceId] });
    },
  });

  const createRequestMut = useMutation({
    mutationFn: (input: CreateRequestInput) => createRequest(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["requests", workspaceId] });
    },
  });

  const handleCreateFolder = () => {
    if (!workspaceId) return;
    createFolderMut.mutate({
      workspace_id: workspaceId,
      parent_folder_id: null,
      name: `New Folder ${Date.now() % 1000}`,
    });
  };

  const handleCreateRequest = () => {
    if (!workspaceId) return;
    createRequestMut.mutate({
      workspace_id: workspaceId,
      folder_id: null,
      name: "New Request",
      method: "GET",
      url: "",
      headers: [],
      query_params: [],
      body: { type: "none" },
      auth: { type: "none" },
    });
  };

  if (!workspaceId) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-4 text-center">
        <p className="text-xs text-muted-foreground">
          请先选择或创建一个 Workspace
        </p>
      </div>
    );
  }

  if (foldersQuery.isLoading || requestsQuery.isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-xs text-muted-foreground">加载中…</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex items-center gap-1 border-b px-2 py-1">
        <button
          onClick={handleCreateFolder}
          className="rounded p-1 hover:bg-accent"
          title="新建 Folder"
        >
          <FolderPlus className="h-3.5 w-3.5" />
        </button>
        <button
          onClick={handleCreateRequest}
          className="rounded p-1 hover:bg-accent"
          title="新建 Request"
        >
          <Plus className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* 树 */}
      <div className="flex-1 overflow-auto">
        {treeData.length === 0 ? (
          <div className="p-4 text-center">
            <p className="text-xs text-muted-foreground">
              Collection 为空，点击上方按钮新建
            </p>
          </div>
        ) : (
          <Tree<FlatNode>
            data={treeData}
            idAccessor="id"
            childrenAccessor="children"
            width="100%"
            height={400}
            indent={12}
            rowHeight={24}
            openByDefault
            disableDrag
            disableDrop
            disableEdit
          >
            {Node}
          </Tree>
        )}
      </div>
    </div>
  );
}
