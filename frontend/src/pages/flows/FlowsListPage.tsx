import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { create, useStore } from "zustand";
import { GitBranch, Plus, Pencil, Trash2 } from "lucide-react";

import { useAgent } from "@/hooks/useAgent";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { cn } from "@/lib/utils";
import { AGENT_BASE_URL } from "@/lib/agent";

import { basename, isDirectChildOfFlow, isFlowNode, nextFlowName } from "./flow-model";

/** Stable empty store used as a fallback before the GraphStore connects. */
const emptyStore = create(() => ({
  nodes: new Map() as Map<string, import("@sys/agent-client").NodeSnapshot>,
  links: new Map() as Map<string, import("@sys/agent-client").Link>,
}));

/**
 * Landing page for /flows — lists every `sys.core.flow` container node
 * with a node/link count and an open-in-editor action. The editor at
 * /flows/edit/:path* handles a single flow at a time.
 */
export function FlowsListPage() {
  const navigate = useNavigate();
  const agent = useAgent();
  const graphStore = useGraphStoreOptional();

  // Eagerly expand root so the GraphStore fetches and caches top-level flows.
  // The store is null until the agent handshake resolves, so we watch for it.
  useEffect(() => {
    if (!graphStore) return;
    graphStore.getState().expand("/");
  }, [graphStore]);

  const activeStore = graphStore ?? emptyStore;
  const nodeMap = useStore(activeStore, (s) => s.nodes);
  const linkMap = useStore(activeStore, (s) => s.links);

  const nodes = useMemo(() => [...nodeMap.values()], [nodeMap]);
  const links = useMemo(() => [...linkMap.values()], [linkMap]);

  const [filter, setFilter] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const rows = useMemo(() => {
    const flows = nodes.filter(isFlowNode);
    return flows
      .map((flow) => {
        const children = nodes.filter((n) => isDirectChildOfFlow(n, flow.path));
        const childIds = new Set(children.map((n) => n.id));
        const linkCount = links.filter(
          (l) => childIds.has(l.source.node_id) && childIds.has(l.target.node_id),
        ).length;
        return {
          id: flow.id,
          path: flow.path,
          name: basename(flow.path),
          lifecycle: flow.lifecycle,
          nodeCount: children.length,
          linkCount,
        };
      })
      .filter((row) => {
        if (!filter.trim()) return true;
        const t = filter.trim().toLowerCase();
        return (
          row.name.toLowerCase().includes(t) ||
          row.path.toLowerCase().includes(t)
        );
      })
      .sort((a, b) => a.path.localeCompare(b.path));
  }, [nodes, links, filter]);

  const createFlowMutation = useMutation({
    mutationFn: async () => {
      const name = nextFlowName(nodes.filter(isFlowNode).map((f) => f.path));
      const created = await agent.data!.nodes.createNode({
        parent: "/",
        kind: "sys.core.flow",
        name,
      });
      return created.path;
    },
    onSuccess: (path) => {
      // GraphStore will update reactively from the SSE node_created event.
      setErrorMessage(null);
      navigate(`/flows/edit${path}`);
    },
    onError: (error) => setErrorMessage(formatError(error)),
  });

  const deleteFlowMutation = useMutation({
    mutationFn: async (path: string) => {
      await agent.data!.nodes.removeNode(path);
    },
    onSuccess: async () => {
      // GraphStore will update reactively from the SSE node_removed event.
      setErrorMessage(null);
    },
    onError: (error) => setErrorMessage(formatError(error)),
  });

  if (!graphStore || agent.isPending) {
    return <CenteredMessage title="Loading flows…" detail={AGENT_BASE_URL} />;
  }

  if (agent.isError) {
    return (
      <CenteredMessage
        title="Could not reach the agent"
        detail={`${AGENT_BASE_URL} — ${formatError(agent.error)}`}
      />
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <header className="flex items-center gap-3">
        <GitBranch size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Flows</h1>
        <span className="text-xs text-muted-foreground">
          One row per <code>sys.core.flow</code> container.
        </span>
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter…"
          className="ml-auto w-48 rounded-md border border-input bg-background px-3 py-1 text-sm outline-none"
        />
        <button
          type="button"
          onClick={() => createFlowMutation.mutate()}
          disabled={createFlowMutation.isPending}
          className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
        >
          <Plus size={14} />
          {createFlowMutation.isPending ? "Creating…" : "New flow"}
        </button>
      </header>

      {errorMessage && (
        <p className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
          {errorMessage}
        </p>
      )}

      {rows.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="flex flex-col items-center gap-2 text-muted-foreground">
            <p className="text-sm">
              {filter.trim() ? "No flows match your filter." : "No flows yet."}
            </p>
            {!filter.trim() && (
              <p className="text-xs">
                Click <strong>New flow</strong> or seed one via{" "}
                <code>POST /api/v1/seed</code>.
              </p>
            )}
          </div>
        </div>
      ) : (
        <div className="overflow-hidden rounded-md border border-border">
          <table className="w-full text-sm">
            <thead className="bg-muted/50 text-left text-xs uppercase text-muted-foreground">
              <tr>
                <th className="px-4 py-2 font-medium">Name</th>
                <th className="px-4 py-2 font-medium">Path</th>
                <th className="px-4 py-2 font-medium">Lifecycle</th>
                <th className="px-4 py-2 font-medium text-right">Nodes</th>
                <th className="px-4 py-2 font-medium text-right">Links</th>
                <th className="px-4 py-2 font-medium"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {rows.map((row) => (
                <tr
                  key={row.id}
                  onClick={() => navigate(`/flows/edit${row.path}`)}
                  className="cursor-pointer transition-colors hover:bg-accent"
                >
                  <td className="px-4 py-2 font-medium">{row.name}</td>
                  <td className="px-4 py-2 font-mono text-xs text-muted-foreground">
                    {row.path}
                  </td>
                  <td className="px-4 py-2">
                    <LifecycleBadge lifecycle={row.lifecycle} />
                  </td>
                  <td className="px-4 py-2 text-right font-mono text-xs">
                    {row.nodeCount}
                  </td>
                  <td className="px-4 py-2 text-right font-mono text-xs">
                    {row.linkCount}
                  </td>
                  <td className="px-4 py-2">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          navigate(`/flows/edit${row.path}`);
                        }}
                        className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-background"
                        title="Open in editor"
                      >
                        <Pencil size={12} />
                        Open
                      </button>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          if (
                            confirm(
                              `Delete flow ${row.path}? This removes the container and all child nodes.`,
                            )
                          ) {
                            deleteFlowMutation.mutate(row.path);
                          }
                        }}
                        className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                        title="Delete flow"
                      >
                        <Trash2 size={12} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function LifecycleBadge({ lifecycle }: { lifecycle: string }) {
  const colour: Record<string, string> = {
    created: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
    active: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
    disabled: "bg-muted text-muted-foreground",
    fault: "bg-destructive/15 text-destructive",
  };
  return (
    <span
      className={cn(
        "rounded-full px-2 py-0.5 text-xs font-medium",
        colour[lifecycle] ?? "bg-muted text-muted-foreground",
      )}
    >
      {lifecycle}
    </span>
  );
}

function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
      <p className="text-sm">{title}</p>
      <p className="max-w-md text-xs font-mono">{detail}</p>
    </div>
  );
}

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}
