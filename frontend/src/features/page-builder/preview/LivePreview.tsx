// Live preview pane — renders the draft through the same SDUI stack
// the runtime uses. Read-only against the store: never writes, never
// fetches. Persistence and validation live elsewhere.
//
// PR 1 does a local parse of the draft's `layoutText` and renders it
// through `SduiProvider`. PR 2 swaps the trigger from "parse locally"
// to "use the dry-run + resolve result" so the preview reflects what
// the server would actually produce.

import { useMemo } from "react";
import { useBuilderStore } from "../store/builder-store.js";
import { SduiProvider, type CustomRegistry } from "@/sdui/context";
import { Renderer } from "@/sdui/Renderer";
import type { UiComponent, UiActionResponse } from "@sys/agent-client";

const emptyRegistry: CustomRegistry = new Map();

interface ParsedLayout {
  kind: "ok";
  root: UiComponent;
}

interface ParsedError {
  kind: "error";
  message: string;
}

type ParseResult = ParsedLayout | ParsedError;

function parseLayout(text: string): ParseResult {
  if (!text.trim()) return { kind: "error", message: "Empty layout" };
  try {
    const v = JSON.parse(text) as unknown;
    if (
      typeof v === "object" &&
      v !== null &&
      "root" in v &&
      typeof (v as { root: unknown }).root === "object"
    ) {
      return { kind: "ok", root: (v as { root: UiComponent }).root };
    }
    return { kind: "error", message: "Missing `root` component" };
  } catch (e) {
    return { kind: "error", message: e instanceof Error ? e.message : String(e) };
  }
}

export function LivePreview() {
  const draft = useBuilderStore((s) => s.draft);
  const parsed = useMemo<ParseResult | null>(
    () => (draft ? parseLayout(draft.layoutText) : null),
    [draft],
  );

  // PR 1: preview does not dispatch actions (the builder's preview is
  // a render-only surface until Stage 3). Provide a no-op so the
  // context contract holds.
  const noopDispatch = async (): Promise<UiActionResponse> => ({ type: "none" });

  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  if (parsed?.kind === "error") {
    return (
      <div className="p-6 text-sm">
        <p className="mb-1 font-semibold text-destructive">Preview unavailable</p>
        <p className="text-muted-foreground">{parsed.message}</p>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto p-4">
      <SduiProvider
        dispatchAction={noopDispatch}
        customRegistry={emptyRegistry}
        pageState={{}}
        setPageState={() => {}}
        treeQueryKey={["page-builder-preview", draft.nodeId] as const}
      >
        {parsed && <Renderer node={parsed.root} />}
      </SduiProvider>
    </div>
  );
}
