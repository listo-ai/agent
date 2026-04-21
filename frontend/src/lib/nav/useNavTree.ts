import { useQuery } from "@tanstack/react-query";
import type { UiNavNode } from "@listo/agent-client";
import { useAgent } from "@/hooks/useAgent";
import { formatError } from "@/lib/utils";
import type { NavNode, NavTreeState } from "./types";

// ---------------------------------------------------------------------------
// Adapter — isolates client schema from the rest of the UI
// ---------------------------------------------------------------------------

function toNavNode(raw: UiNavNode): NavNode {
  return {
    id: raw.id,
    title: raw.title,
    path: raw.path,
    icon: raw.icon,
    order: raw.order,
    frameAlias: raw.frame_alias,
    children: raw.children
      .map(toNavNode)
      .sort((a, b) => (a.order ?? 0) - (b.order ?? 0)),
  };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Fetches the `ui.nav` subtree rooted at `rootId` from
 * `GET /api/v1/ui/nav?root=<rootId>`.
 *
 * Pass `undefined` to skip the call (e.g. when in admin mode).
 *
 * @param rootId  Graph id of the root `ui.nav` node, or undefined to skip.
 */
export function useNavTree(rootId: string | undefined): NavTreeState {
  const agent = useAgent();

  const query = useQuery<NavNode>({
    queryKey: ["nav-tree", rootId],
    queryFn: async () => {
      const raw = await agent.data!.ui.nav(rootId!);
      return toNavNode(raw);
    },
    enabled: agent.data !== undefined && rootId !== undefined,
    staleTime: 30_000,
  });

  if (rootId === undefined) {
    return { status: "unconfigured", root: null, errorDetail: null, refetch: query.refetch };
  }
  if (query.isPending || agent.isPending) {
    return { status: "loading", root: null, errorDetail: null, refetch: query.refetch };
  }
  if (query.isError) {
    return {
      status: "error",
      root: null,
      errorDetail: formatError(query.error),
      refetch: query.refetch,
    };
  }
  return {
    status: "ready",
    root: query.data ?? null,
    errorDetail: null,
    refetch: query.refetch,
  };
}
