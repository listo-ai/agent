/**
 * Lists every `ui.page` node in the graph. Pages are nodes — this is a
 * filtered view over `/api/v1/nodes`, not a dedicated endpoint.
 *
 * The list intentionally re-queries on every mount (`staleTime: 0`)
 * and exposes a manual refresh — SSE-driven live invalidation will
 * land when the graph store becomes the authoritative source for this
 * surface.
 */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { FileText, ArrowRight, Pencil, Plus, RefreshCw } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@listo/ui-kit";
import { agentPromise } from "@listo/ui-core";
import type { NodeSnapshot } from "@listo/agent-client";

const UI_PAGE_KIND = "ui.page";
const LAYOUT_SLOT = "layout";

function nameFromPath(path: string) {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function startingLayout(name: string): unknown {
  return {
    ir_version: 1,
    root: {
      type: "page",
      id: "root",
      title: name,
      children: [
        { type: "heading", level: 2, content: name },
        { type: "text", content: "Edit this layout to build your page." },
      ],
    },
  };
}

function useUiPages() {
  return useQuery<NodeSnapshot[]>({
    queryKey: ["ui-pages"],
    queryFn: async () => {
      const client = await agentPromise;
      const resp = await client.nodes.getNodesPage({
        filter: `kind=="${UI_PAGE_KIND}"`,
        size: 200,
      });
      return resp.data;
    },
    staleTime: 0,
    refetchOnWindowFocus: true,
  });
}

export function PagesListPage() {
  const { data: pages, isLoading, isError, isFetching } = useUiPages();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["ui-pages"] });

  const createPage = async () => {
    const raw = window.prompt("New page name (letters, digits, hyphen, underscore):");
    const name = raw?.trim();
    if (!name) return;
    setCreating(true);
    try {
      const client = await agentPromise;
      const { id, path } = await client.nodes.createNode({
        parent: "/",
        kind: UI_PAGE_KIND,
        name,
      });
      await client.slots.writeSlot(path, LAYOUT_SLOT, startingLayout(name));
      queryClient.invalidateQueries({ queryKey: ["ui-pages"] });
      navigate(`/pages/${encodeURIComponent(id)}/edit`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      alert(`Failed to create page: ${msg}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-center gap-3">
        <FileText size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Pages</h1>
        <span className="text-xs text-muted-foreground">
          {pages ? `${pages.length} ui.page node${pages.length === 1 ? "" : "s"}` : ""}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            onClick={refresh}
            className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
            title="Refresh"
          >
            <RefreshCw size={12} className={isFetching ? "animate-spin" : ""} />
            Refresh
          </button>
          <button
            type="button"
            onClick={createPage}
            disabled={creating}
            className="flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus size={12} />
            New page
          </button>
        </div>
      </header>

      {isLoading && (
        <p className="text-sm text-muted-foreground">Loading pages…</p>
      )}

      {isError && (
        <p className="text-sm text-destructive">Failed to load ui.page nodes.</p>
      )}

      {pages && pages.length === 0 && (
        <div className="flex flex-1 items-center justify-center">
          <Card className="max-w-md text-center">
            <CardHeader>
              <FileText
                size={32}
                className="mx-auto mb-2 text-muted-foreground"
              />
              <CardTitle>No pages yet</CardTitle>
              <CardDescription>
                A page is a UI layout — a tree of widgets (text, chart,
                table, form, …) stored in a <code>ui.page</code> node.
                Create one to get started.
              </CardDescription>
            </CardHeader>
            <CardContent className="flex flex-col items-center gap-3">
              <button
                type="button"
                onClick={createPage}
                disabled={creating}
                className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              >
                <Plus size={14} />
                Create your first page
              </button>
              <p className="text-xs text-muted-foreground">
                Want to do it from the terminal instead? Run{" "}
                <code className="rounded bg-muted px-1">agent nodes create / ui.page my-page</code>.
              </p>
            </CardContent>
          </Card>
        </div>
      )}

      {pages && pages.length > 0 && (
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {pages.map((page) => (
            <Card key={page.id} className="transition-colors hover:border-primary/50">
              <CardHeader className="pb-2">
                <div className="flex items-start justify-between gap-2">
                  <Link
                    to={`/ui/${encodeURIComponent(page.id)}`}
                    className="group flex min-w-0 flex-1 flex-col gap-0.5"
                  >
                    <CardTitle className="flex items-center gap-2 text-sm">
                      <FileText size={14} className="text-muted-foreground" />
                      {nameFromPath(page.path)}
                      <ArrowRight
                        size={12}
                        className="shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
                      />
                    </CardTitle>
                    <CardDescription className="truncate text-xs">
                      {page.path}
                    </CardDescription>
                  </Link>
                  <Link
                    to={`/pages/${encodeURIComponent(page.id)}/edit`}
                    className="shrink-0 rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                    title="Edit layout"
                    aria-label={`Edit ${nameFromPath(page.path)}`}
                  >
                    <Pencil size={14} />
                  </Link>
                </div>
              </CardHeader>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
