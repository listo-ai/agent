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
import type { UiActionResponse, UiResolveResponse } from "@sys/agent-client";

// The global custom-renderer registry.  Plugins populate this at load time.
export const globalCustomRegistry: CustomRegistry = new Map();

export function SduiPage() {
  const { pageRef } = useParams<{ pageRef: string }>();
  const [pageState, setPageState] = useState<Record<string, unknown>>({});

  const { data, isLoading, isError, error } = useQuery<UiResolveResponse>({
    queryKey: ["sdui-resolve", pageRef, pageState],
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
          Failed to resolve page: {String(error ?? "unknown error")}
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

  return (
    <SduiProvider
      dispatchAction={dispatchAction}
      customRegistry={globalCustomRegistry}
      pageState={pageState}
      setPageState={mergePageState}
    >
      <Renderer node={data.render.root} />
    </SduiProvider>
  );
}
