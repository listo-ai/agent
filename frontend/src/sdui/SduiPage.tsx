/**
 * Route: `/ui/:pageRef`
 *
 * Resolves a `ui.page` via `POST /api/v1/ui/resolve`, wraps the result
 * in SduiProvider, and hands the tree to the Renderer.  The `pageRef`
 * param is the node id or path of the page to render.
 */
import { useState, useMemo } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { SduiProvider, type CustomRegistry } from "./context";
import { Renderer } from "./Renderer";
import { useSubscriptions } from "./useSubscriptions";
import { checkIrVersion } from "./capability";
import type { UiActionResponse, UiResolveResponse } from "@listo/agent-client";

// The global custom-renderer registry.  Plugins populate this at load time.
export const globalCustomRegistry: CustomRegistry = new Map();

function formatSduiError(e: unknown): string {
  if (!e) return "unknown error";
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export function SduiPage() {
  const { pageRef } = useParams<{ pageRef: string }>();
  const [pageState, setPageState] = useState<Record<string, unknown>>({});

  const queryKey = ["sdui-resolve", pageRef, pageState] as const;
  const { data, isLoading, isError, error } = useQuery<UiResolveResponse>({
    queryKey,
    queryFn: async () => {
      const client = await agentPromise;
      return client.ui.resolve({
        page_ref: decodeURIComponent(pageRef ?? ""),
        stack: [],
        page_state: pageState,
        dry_run: false,
        user_claims: {},
      });
    },
    staleTime: 0,
    enabled: !!pageRef,
  });

  // Live-update on every subscribed slot. `data` is a discriminated
  // union — only the Ok variant carries subscriptions.
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

  const mergePageState = useMemo(
    () => (patch: Record<string, unknown>) =>
      setPageState((prev) => ({ ...prev, ...patch })),
    [],
  );

  if (!pageRef) {
    return <div className="p-6 text-sm text-muted-foreground">No page ref provided.</div>;
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-sm text-muted-foreground">Resolving…</span>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="p-6">
        <p className="text-sm text-destructive">
          Failed to resolve page: {formatSduiError(error)}
        </p>
      </div>
    );
  }

  if ("errors" in data) {
    return (
      <div className="p-6">
        <p className="mb-2 text-sm font-semibold">Dry-run issues:</p>
        <ul className="flex flex-col gap-1">
          {data.errors.map((e, i) => (
            <li key={i} className="text-sm text-destructive">
              <code>{e.location}</code>: {e.message}
            </li>
          ))}
        </ul>
      </div>
    );
  }

  const mismatch = checkIrVersion(data.render);
  if (mismatch) {
    return (
      <div className="p-6">
        <p className="text-sm text-destructive">
          Capability mismatch: server emitted <code>ir_version={mismatch.received}</code>,
          client supports up to <code>{mismatch.supported}</code>. Upgrade the
          frontend to render this page.
        </p>
      </div>
    );
  }

  return (
    <SduiProvider
      dispatchAction={dispatchAction}
      customRegistry={globalCustomRegistry}
      pageState={pageState}
      setPageState={mergePageState}
      treeQueryKey={queryKey}
    >
      <Renderer node={data.render.root} />
    </SduiProvider>
  );
}
