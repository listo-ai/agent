/**
 * Dashboard page — lists all `ui.page` nodes in the graph, each linking
 * to `/ui/:nodeId` which renders them through the SDUI renderer.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { LayoutDashboard, FileText, ArrowRight } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui";
import { agentPromise } from "@/lib/agent";
import type { NodeSnapshot } from "@sys/agent-client";

function nameFromPath(path: string) {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function useUiPages() {
  return useQuery<NodeSnapshot[]>({
    queryKey: ["ui-pages"],
    queryFn: async () => {
      const client = await agentPromise;
      const resp = await client.nodes.getNodesPage({
        filter: 'kind=="ui.page"',
        size: 200,
      });
      return resp.data;
    },
    staleTime: 30_000,
  });
}

export function DashboardPage() {
  const { data: pages, isLoading, isError } = useUiPages();

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-center gap-3">
        <LayoutDashboard size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Dashboard</h1>
      </header>

      {isLoading && (
        <p className="text-sm text-muted-foreground">Loading pages…</p>
      )}

      {isError && (
        <p className="text-sm text-destructive">Failed to load ui.page nodes.</p>
      )}

      {pages && pages.length === 0 && (
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>No pages yet</CardTitle>
            <CardDescription>Create your first SDUI page via the CLI</CardDescription>
          </CardHeader>
          <CardContent>
            <pre className="rounded bg-muted p-2 text-xs leading-relaxed">
              {`agent nodes create /dashboards ui.page my-page\nagent slots write /dashboards/my-page layout \\\n  '{"type":"page","ir_version":1,"title":"Hello","children":[{"type":"text","content":"Hello from SDUI!"}]}'`}
            </pre>
          </CardContent>
        </Card>
      )}

      {pages && pages.length > 0 && (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {pages.map((page) => (
            <Link
              key={page.id}
              to={`/ui/${encodeURIComponent(page.id)}`}
              className="group block"
            >
              <Card className="transition-colors hover:border-primary/50">
                <CardHeader className="pb-2">
                  <div className="flex items-start justify-between gap-2">
                    <CardTitle className="flex items-center gap-2 text-sm">
                      <FileText size={14} className="text-muted-foreground" />
                      {nameFromPath(page.path)}
                    </CardTitle>
                    <ArrowRight
                      size={14}
                      className="mt-0.5 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
                    />
                  </div>
                  <CardDescription className="text-xs">{page.path}</CardDescription>
                </CardHeader>
              </Card>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
