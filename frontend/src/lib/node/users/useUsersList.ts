import { useQuery } from "@tanstack/react-query";
import type { User } from "@listo/agent-client";
import { useAgent } from "@listo/ui-core";
import { formatError } from "@/lib/utils";
import type { UserNode, UsersListParams, UsersListState } from "./types";

// ---------------------------------------------------------------------------
// Adapter — keep API schema concerns out of the UI layer
// ---------------------------------------------------------------------------

function toUserNode(u: User): UserNode {
  return {
    id: u.id,
    path: u.path,
    displayName: u.display_name,
    email: u.email,
    enabled: u.enabled,
    tags: { labels: u.tags.labels, kv: u.tags.kv },
  };
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Fetches the `sys.auth.user` node list from `GET /api/v1/users`.
 *
 * UI components receive a pure `UsersListState` — they never import the
 * client schema types directly.
 *
 * @param params  Optional filter / sort / pagination params.
 */
export function useUsersList(params: UsersListParams = {}): UsersListState {
  const { filter, sort, page, size } = params;
  const agent = useAgent();

  const query = useQuery<UserNode[]>({
    queryKey: ["users", filter, sort, page, size],
    queryFn: async () => {
      const apiParams: { filter?: string; sort?: string; page?: number; size?: number } = {};
      if (filter !== undefined) apiParams.filter = filter;
      if (sort !== undefined) apiParams.sort = sort;
      if (page !== undefined) apiParams.page = page;
      if (size !== undefined) apiParams.size = size;
      const raw = await agent.data!.users.list(apiParams);
      return raw.map(toUserNode);
    },
    enabled: agent.data !== undefined,
  });

  if (query.isPending || agent.isPending) {
    return { status: "loading", users: [], errorDetail: null, refetch: query.refetch };
  }
  if (query.isError) {
    return {
      status: "error",
      users: [],
      errorDetail: formatError(query.error),
      refetch: query.refetch,
    };
  }
  return {
    status: "ready",
    users: query.data ?? [],
    errorDetail: null,
    refetch: query.refetch,
  };
}
