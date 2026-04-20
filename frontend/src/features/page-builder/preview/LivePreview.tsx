// Live preview pane.
//
// Unlike `/ui/:id` (which resolves the persisted slot), the builder's
// preview resolves the in-flight editor buffer — we POST the parsed
// draft as an inline `layout` override to `/ui/resolve`. The response
// includes the same render tree *and* the subscription plan derived
// from the candidate buffer, so live ticks invalidate exactly the
// widgets that would tick in production.

import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { useBuilderStore } from "../store/builder-store.js";
import { validateLayout } from "../model/validate-layout.js";
import { SduiProvider, type CustomRegistry } from "@/sdui/context";
import { Renderer } from "@/sdui/Renderer";
import { useSubscriptions } from "@/sdui/useSubscriptions";
import type { UiActionResponse, UiResolveResponse } from "@sys/agent-client";

const emptyRegistry: CustomRegistry = new Map();
const DEBOUNCE_MS = 250;

/**
 * The HTTP transport wraps server errors as
 * `{kind:"HttpError", status, message}`. Its `message` is the raw
 * response body, which the agent returns as `{"error":"..."}`. Dig
 * the human-readable `error` field out for display.
 */
function extractServerMessage(raw: string): string {
  try {
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object" && typeof parsed.error === "string") {
      return parsed.error;
    }
  } catch {
    /* fall through */
  }
  return raw;
}

export function LivePreview() {
  const draft = useBuilderStore((s) => s.draft);
  const [debouncedText, setDebouncedText] = useState<string | undefined>(
    draft?.layoutText,
  );
  const queryClient = useQueryClient();

  // Debounce editor buffer → what we actually send to the server.
  useEffect(() => {
    if (draft?.layoutText === undefined) return;
    const t = window.setTimeout(() => setDebouncedText(draft.layoutText), DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [draft?.layoutText]);

  const parsed = useMemo(
    () => (debouncedText !== undefined ? validateLayout(debouncedText) : null),
    [debouncedText],
  );

  const [pageState, setPageState] = useState<Record<string, unknown>>({});
  const mergePageState = useMemo(
    () => (patch: Record<string, unknown>) =>
      setPageState((prev) => ({ ...prev, ...patch })),
    [],
  );

  const queryKey = [
    "page-builder-resolve",
    draft?.nodeId ?? "",
    debouncedText ?? "",
    pageState,
  ] as const;

  const { data, isError, error, isFetching } = useQuery<UiResolveResponse | null>({
    queryKey,
    enabled: !!draft?.nodeId && parsed?.ok === true,
    staleTime: 0,
    queryFn: async () => {
      if (!draft || parsed?.ok !== true) return null;
      const client = await agentPromise;
      return client.ui.resolve({
        page_ref: draft.nodeId,
        stack: [],
        page_state: pageState,
        dry_run: false,
        user_claims: {},
        layout: parsed.value,
      });
    },
  });

  const subscriptions =
    data && "subscriptions" in data ? data.subscriptions : undefined;
  useSubscriptions(queryKey, subscriptions);

  const dispatchAction = useMemo(
    () => async (handler: string, args?: unknown): Promise<UiActionResponse> => {
      const client = await agentPromise;
      return client.ui.action({
        handler,
        args: args ?? null,
        context: { stack: [], page_state: pageState },
      });
    },
    [pageState],
  );

  // Surface a preview-invalidation helper for future manual refresh.
  const _refresh = () => queryClient.invalidateQueries({ queryKey });
  void _refresh;

  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  if (parsed && !parsed.ok) {
    return (
      <div className="p-6 text-sm">
        <p className="mb-1 font-semibold text-destructive">Preview unavailable</p>
        <p className="text-muted-foreground">{parsed.issues[0]?.message}</p>
      </div>
    );
  }

  if (isError) {
    const msg = error instanceof Error ? error.message : String(error);
    return (
      <div className="p-6 text-sm">
        <p className="mb-1 font-semibold text-destructive">Preview failed</p>
        <p className="whitespace-pre-wrap break-words text-xs text-muted-foreground">
          {extractServerMessage(msg)}
        </p>
        <p className="mt-2 text-xs text-muted-foreground">
          Fix the layout in the editor — check the validation strip
          below or the Network tab for details.
        </p>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        {isFetching ? "Resolving…" : "No preview yet."}
      </div>
    );
  }

  if ("errors" in data) {
    return (
      <div className="p-6 text-sm">
        <p className="mb-1 font-semibold text-destructive">Preview failed</p>
        <ul className="space-y-1 text-xs">
          {data.errors.map((e, i) => (
            <li key={i}>
              <code className="text-muted-foreground">{e.location}</code>: {e.message}
            </li>
          ))}
        </ul>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto p-4">
      <SduiProvider
        dispatchAction={dispatchAction}
        customRegistry={emptyRegistry}
        pageState={pageState}
        setPageState={mergePageState}
        treeQueryKey={queryKey}
      >
        <Renderer node={data.render.root} />
      </SduiProvider>
    </div>
  );
}
