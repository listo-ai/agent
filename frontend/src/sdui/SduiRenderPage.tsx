/**
 * Route: `/render/:targetId` (optional `?view=<id>`)
 *
 * Thin wrapper around `GET /api/v1/ui/render` — looks up the target
 * node's kind, picks the matching `views` entry on the kind manifest,
 * and hands the resolved tree to the renderer. This is the S5 shipping
 * surface: clicking any node in Studio shows its default view without
 * any authored `ui.page`.
 */
import { useState, useMemo } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { SduiProvider, type CustomRegistry } from "./context";
import { Renderer } from "./Renderer";
import { useSubscriptions } from "./useSubscriptions";
import { checkIrVersion } from "./capability";
import type { UiActionResponse, UiResolveResponse } from "@sys/agent-client";

export const globalCustomRegistry: CustomRegistry = new Map();

function formatError(e: unknown): string {
  if (!e) return "unknown error";
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export function SduiRenderPage() {
  const { targetId } = useParams<{ targetId: string }>();
  const [search] = useSearchParams();
  const view = search.get("view") ?? undefined;
  const [pageState, setPageState] = useState<Record<string, unknown>>({});

  const queryKey = ["sdui-render", targetId, view] as const;
  const { data, isLoading, isError, error } = useQuery<UiResolveResponse>({
    queryKey,
    queryFn: async () => {
      const client = await agentPromise;
      return client.ui.render(decodeURIComponent(targetId ?? ""), view);
    },
    staleTime: 0,
    enabled: !!targetId,
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
        context: { target: targetId, stack: [], page_state: pageState },
      });
    },
    [pageState, targetId],
  );

  const mergePageState = useMemo(
    () => (patch: Record<string, unknown>) =>
      setPageState((prev) => ({ ...prev, ...patch })),
    [],
  );

  if (!targetId) {
    return (
      <div className="p-6 text-sm text-muted-foreground">
        No target id provided.
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <span className="text-sm text-muted-foreground">Rendering…</span>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="p-6">
        <p className="text-sm text-destructive">
          Failed to render node: {formatError(error)}
        </p>
      </div>
    );
  }

  if ("errors" in data) {
    return (
      <div className="p-6">
        <p className="mb-2 text-sm font-semibold">Render issues:</p>
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
