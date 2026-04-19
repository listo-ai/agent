// Split-pane shell for one page-builder session. Mounted at
// `/pages/:id/edit`. Fetches the ui.page node, hydrates the store,
// and composes the two pre-existing panels.
//
// This component is the only layer allowed to know about both
// react-query and the store — that's the persistence seam. In PR 3
// the fetch + save logic moves into `persistence/` so this shell
// collapses to just composition.

import { useEffect } from "react";
import { useParams, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { agentPromise } from "@/lib/agent";
import { useBuilderStore } from "./store/builder-store";
import { LivePreview } from "./preview/LivePreview";
import { EditorPane } from "./panels/EditorPane";
import { ValidationList } from "./panels/ValidationList";
import { useValidator } from "./persistence/use-validator";
import type { DraftPage } from "./model/types";
import type { NodeSnapshot } from "@sys/agent-client";

const LAYOUT_SLOT = "layout";

async function loadDraft(nodeId: string): Promise<DraftPage> {
  const client = await agentPromise;
  // Pull the node by id through the list endpoint — `/api/v1/node` is
  // path-only today, and this path is equally versioned + cheap for
  // single-row lookups.
  const page = await client.nodes.getNodesPage({
    filter: `id=="${nodeId}"`,
    size: 1,
  });
  const snap = page.data[0];
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
  const hydrate = useBuilderStore((s) => s.hydrate);
  const reset = useBuilderStore((s) => s.reset);
  useValidator();

  const { data, isLoading, isError, error } = useQuery({
    queryKey: ["page-builder", id],
    queryFn: () => loadDraft(id!),
    enabled: !!id,
    staleTime: 0,
  });

  useEffect(() => {
    if (data) hydrate(data);
    return () => reset();
  }, [data, hydrate, reset]);

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
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-border px-4 py-2">
        <Link to="/pages" className="text-muted-foreground hover:text-foreground">
          <ArrowLeft size={16} />
        </Link>
        <h1 className="text-sm font-semibold">{data.nodePath}</h1>
        <span className="text-xs text-muted-foreground">gen {data.baseGeneration}</span>
      </header>
      <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-border">
        <EditorPane />
        <LivePreview />
      </div>
      <ValidationList />
    </div>
  );
}
