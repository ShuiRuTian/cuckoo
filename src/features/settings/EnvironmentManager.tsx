/**
 * 环境变量管理界面 + 环境切换（`spec.md` 6.2 节、`plan.md` M1.4）。
 *
 * M1 阶段实现：
 * - 环境切换下拉框（顶部工具栏）
 * - 环境变量 Key-Value 列表编辑（使用 KeyValueEditor）
 * - 新建/删除环境
 *
 * 数据流：
 * - listEnvironments → 下拉框选项
 * - updateEnvironment → 保存变量修改
 * - createEnvironment → 新建环境
 */

import { useAtom } from "jotai";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import {
  listEnvironments,
  createEnvironment,
  updateEnvironment,
  deleteEnvironment,
  type EnvVariable,
  type CreateEnvironmentInput,
  type UpdateEnvironmentInput,
} from "@/lib/api/generated";
import { currentWorkspaceIdAtom, currentEnvironmentIdAtom } from "@/state/app";
import { KeyValueEditor } from "@/components/custom/KeyValueEditor";

export function EnvironmentManager() {
  const [workspaceId] = useAtom(currentWorkspaceIdAtom);
  const [envId, setEnvId] = useAtom(currentEnvironmentIdAtom);
  const queryClient = useQueryClient();

  const envsQuery = useQuery({
    queryKey: ["environments", workspaceId],
    queryFn: () => listEnvironments(workspaceId!),
    enabled: !!workspaceId,
  });

  const updateMut = useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateEnvironmentInput }) =>
      updateEnvironment(id, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["environments", workspaceId] });
    },
  });

  const createMut = useMutation({
    mutationFn: (input: CreateEnvironmentInput) => createEnvironment(input),
    onSuccess: (data) => {
      queryClient.invalidateQueries({ queryKey: ["environments", workspaceId] });
      setEnvId(data.id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => deleteEnvironment(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["environments", workspaceId] });
      setEnvId(null);
    },
  });

  const selectedEnv = envsQuery.data?.find((e) => e.id === envId) ?? null;

  const handleCreate = () => {
    if (!workspaceId) return;
    createMut.mutate({
      workspace_id: workspaceId,
      name: `Environment ${Date.now() % 1000}`,
      variables: [],
    });
  };

  const handleDelete = () => {
    if (envId) deleteMut.mutate(envId);
  };

  const handleVariablesChange = (variables: EnvVariable[]) => {
    if (!selectedEnv) return;
    updateMut.mutate({
      id: selectedEnv.id,
      input: { name: null, variables },
    });
  };

  if (!workspaceId) {
    return (
      <div className="p-2 text-xs text-muted-foreground">
        请先选择 Workspace
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2 p-2">
      {/* 环境切换下拉框 + 操作按钮 */}
      <div className="flex items-center gap-1">
        <select
          value={envId ?? ""}
          onChange={(e) => setEnvId(e.target.value || null)}
          className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
        >
          <option value="">No Environment</option>
          {envsQuery.data?.map((env) => (
            <option key={env.id} value={env.id}>
              {env.name}
            </option>
          ))}
        </select>
        <button
          onClick={handleCreate}
          className="rounded border border-border p-1 hover:bg-accent"
          title="新建环境"
        >
          <Plus className="h-3 w-3" />
        </button>
        {envId && (
          <button
            onClick={handleDelete}
            className="rounded border border-border p-1 hover:bg-accent"
            title="删除环境"
          >
            <Trash2 className="h-3 w-3" />
          </button>
        )}
      </div>

      {/* 变量编辑器 */}
      {selectedEnv && (
        <KeyValueEditor<EnvVariable>
          entries={selectedEnv.variables}
          toEntries={(items) => items.map((v) => ({ key: v.key, value: v.value, enabled: v.enabled }))}
          fromEntries={(entries) => entries.map((e) => ({ key: e.key, value: e.value, secret: false, enabled: e.enabled }))}
          onChange={(variables) => handleVariablesChange(variables)}
          keyPlaceholder="variable name"
          valuePlaceholder="value"
        />
      )}
    </div>
  );
}
