import { useState, useCallback } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useAtom } from "jotai";
import { ResizablePanel } from "@/components/custom/ResizablePanel";
import { CollectionTree } from "@/features/collections/CollectionTree";
import { WorkspaceSelector } from "@/features/collections/WorkspaceSelector";
import { RequestEditor } from "@/features/request-builder/RequestEditor";
import { EnvironmentManager } from "@/features/settings/EnvironmentManager";
import { ProxyDashboard } from "@/features/proxy/ProxyDashboard";
import { apiFetch } from "@/lib/api/client";
import type { PongResponse } from "@/lib/api/generated";
import { appModeAtom } from "@/state/app";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

function App() {
  const [mode, setMode] = useAtom(appModeAtom);
  const [pong, setPong] = useState<PongResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handlePing = useCallback(async () => {
    try {
      setError(null);
      const result = await apiFetch<PongResponse>("/api/ping");
      setPong(result);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <div className="flex h-screen flex-col bg-background text-foreground">
        {/* 顶部模式切换 Tab（Client / Proxy），spec.md 6.2 节 */}
        <header className="flex items-center gap-2 border-b px-2 py-1">
          <div className="flex gap-1">
            <button
              className={`rounded px-3 py-1 text-sm font-medium transition-colors ${
                mode === "client"
                  ? "bg-primary text-primary-foreground"
                  : "hover:bg-accent"
              }`}
              onClick={() => setMode("client")}
            >
              Client
            </button>
            <button
              className={`rounded px-3 py-1 text-sm font-medium transition-colors ${
                mode === "proxy"
                  ? "bg-primary text-primary-foreground"
                  : "hover:bg-accent"
              }`}
              onClick={() => setMode("proxy")}
            >
              Proxy
            </button>
          </div>

          {/* Workspace 选择器 */}
          {mode === "client" && <WorkspaceSelector />}

          {/* 环境切换 */}
          {mode === "client" && <EnvironmentManager />}

          {/* M0 端到端闭环验证 */}
          <div className="ml-auto flex items-center gap-2">
            <button
              onClick={handlePing}
              className="rounded border border-border bg-card px-3 py-1 text-sm font-medium shadow-sm hover:bg-accent"
            >
              Ping
            </button>
            {pong && (
              <div className="rounded bg-muted p-2 text-xs">
                <div>message: {pong.message}</div>
                <div>server_time_ms: {pong.server_time_ms}</div>
              </div>
            )}
            {error && (
              <div className="rounded bg-red-100 p-2 text-xs text-red-700 dark:bg-red-900/30 dark:text-red-400">
                {error}
              </div>
            )}
          </div>
        </header>

        {/* 主工作区：左侧边栏 + 右侧主面板（自研 ResizablePanel） */}
        {mode === "client" ? (
          <ResizablePanel
            direction="horizontal"
            className="flex-1"
            initialRatio={0.25}
          >
            {/* 左侧：Collection 树 */}
            <aside className="h-full overflow-hidden border-r bg-secondary/30">
              <CollectionTree />
            </aside>

            {/* 右侧：请求编辑器 */}
            <main className="h-full overflow-hidden">
              <RequestEditor />
            </main>
          </ResizablePanel>
        ) : (
          <ProxyDashboard />
        )}
      </div>
    </QueryClientProvider>
  );
}

export default App;
