// Split-pane shell for one page-builder session. Mounted at
// `/pages/:id/edit`. Fetches the ui.page node, hydrates the store,
// and composes the panels.
//
// This component is the only layer allowed to know about both
// react-query and the store — that's the persistence seam. The
// validator and autosave hooks do their own side effects against
// the store; the shell just mounts them.

import { useCallback, useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Sparkles } from "lucide-react";
import { agentPromise } from "@/lib/agent";
import { useBuilderStore } from "./store/builder-store";
import { LivePreview } from "./preview/LivePreview";
import { EditorPane } from "./panels/EditorPane";
import { ValidationList } from "./panels/ValidationList";
import { ConflictBanner } from "./panels/ConflictBanner";
import { SaveStatus } from "./panels/SaveStatus";
import { ComposePanel } from "./panels/ComposePanel";
import { useValidator } from "./persistence/use-validator";
import { useAutosave } from "./persistence/use-autosave";
import type { DraftPage } from "./model/types";
import type { NodeSnapshot } from "@sys/agent-client";

const LAYOUT_SLOT = "layout";

async function loadDraft(nodeId: string): Promise<DraftPage> {
  const client = await agentPromise;
  // List all ui.page nodes and pick by id. The kind-filter path is
  // proven to work; filtering directly by `id==<uuid>` has been
  // flaky in practice — RSQL dashes seem to confuse some parsers.
  // The list is small in practice, so one extra roundtrip beats a
  // spooky filter bug.
  const resp = await client.nodes.getNodesPage({
    filter: `kind=="ui.page"`,
    size: 500,
  });
  const snap = resp.data.find((n) => n.id === nodeId);
  if (!snap) throw new Error(`No ui.page node with id ${nodeId}`);
  return toDraft(snap);
}

function toDraft(snap: NodeSnapshot): DraftPage {
  const slot = snap.slots.find((s) => s.name === LAYOUT_SLOT);
  const layoutText = slot?.value !== undefined && slot.value !== null
    ? JSON.stringify(slot.value, null, 2)
    : "";
  return {
    nodeId: snap.id,
    nodePath: snap.path,
    layoutText,
    baseGeneration: slot?.generation ?? 0,
  };
}

export function PageBuilderPage() {
  const { id } = useParams<{ id: string }>();
  const queryKey = ["page-builder", id] as const;
  const queryClient = useQueryClient();
  const hydrate = useBuilderStore((s) => s.hydrate);
  const reset = useBuilderStore((s) => s.reset);

  const { data, isLoading, isError, error } = useQuery({
    queryKey,
    queryFn: () => loadDraft(id!),
    enabled: !!id,
    staleTime: 0,
  });

  useEffect(() => {
    if (data) hydrate(data);
    return () => reset();
  }, [data, hydrate, reset]);

  useValidator();
  useAutosave();

  const [composeOpen, setComposeOpen] = useState(false);

  const reload = useCallback(() => {
    queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  if (!id) {
    return <div className="p-6 text-sm text-muted-foreground">No page id provided.</div>;
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-sm text-muted-foreground">Loading page…</span>
      </div>
    );
  }

  if (isError || !data) {
    const msg = error instanceof Error ? error.message : String(error);
    return (
      <div className="p-6">
        <p className="text-sm text-destructive">Failed to load page: {msg}</p>
        <Link to="/pages" className="mt-2 inline-flex text-xs underline">
          Back to pages
        </Link>
      </div>
    );
  }

  return (
    <div className="relative flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Link to="/pages" className="text-muted-foreground hover:text-foreground">
          <ArrowLeft size={16} />
        </Link>
        <h1 className="text-sm font-semibold">{data.nodePath}</h1>
        <div className="ml-auto flex items-center gap-3">
          <SaveStatus />
          <button
            type="button"
            onClick={() => setComposeOpen((v) => !v)}
            aria-pressed={composeOpen}
            className="flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
            title="Generate or edit the layout with AI"
          >
            <Sparkles size={12} />
            Compose
          </button>
        </div>
      </header>
      <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-border">
        <EditorPane />
        <LivePreview />
      </div>
      <ValidationList />
      <ConflictBanner onReload={reload} />
      <ComposePanel open={composeOpen} onClose={() => setComposeOpen(false)} />
    </div>
  );
}
