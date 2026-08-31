/**
 * Workspace 选择器（`spec.md` 6.2 节）。
 *
 * 顶部工具栏组件：列出所有 Workspace，选中后更新 `currentWorkspaceIdAtom`。
 * 支持新建 Workspace。
 */

import { useAtom } from "jotai";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import {
  listWorkspaces,
  createWorkspace,
  type CreateWorkspaceInput,
} from "@/lib/api/generated";
import { currentWorkspaceIdAtom } from "@/state/app";

export function WorkspaceSelector() {
  const [workspaceId, setWorkspaceId] = useAtom(currentWorkspaceIdAtom);
  const queryClient = useQueryClient();

  const workspacesQuery = useQuery({
    queryKey: ["workspaces"],
    queryFn: listWorkspaces,
  });

  const createMut = useMutation({
    mutationFn: (input: CreateWorkspaceInput) => createWorkspace(input),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ["workspaces"] });
      setWorkspaceId(data.id);
    },
  });

  const handleCreate = () => {
    createMut.mutate({
      name: `Workspace ${Date.now() % 1000}`,
      base_headers: [],
      settings: { verify_tls: true, timeout_ms: null },
    });
  };

  return (
    <div className="flex items-center gap-1">
      <select
        value={workspaceId ?? ""}
        onChange={(e) => setWorkspaceId(e.target.value || null)}
        className="rounded border border-border bg-background px-2 py-1 text-xs"
      >
        <option value="">Select Workspace…</option>
        {workspacesQuery.data?.map((ws) => (
          <option key={ws.id} value={ws.id}>
            {ws.name}
          </option>
        ))}
      </select>
      <button
        onClick={handleCreate}
        className="rounded border border-border p-1 hover:bg-accent"
        title="新建 Workspace"
      >
        <Plus className="h-3 w-3" />
      </button>
    </div>
  );
}
