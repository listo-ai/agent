import { useQuery } from "@tanstack/react-query";
import type { AgentClient, Kind, Link, NodeSnapshot } from "@sys/agent-client";

import { agentPromise } from "@/lib/agent";

/**
 * Resolves the singleton `AgentClient`. Never stale — once the promise
 * resolves the client is reused for the lifetime of the app.
 */
export function useAgent() {
  return useQuery<AgentClient>({
    queryKey: ["agent"],
    queryFn: () => agentPromise,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/**
 * Top-level nodes query. Keyed by path; refetched on mount / focus via
 * the defaults set in [`QueryProvider`](../providers/query.tsx).
 */
export function useNodes() {
  const agent = useAgent();
  return useQuery<NodeSnapshot[]>({
    queryKey: ["nodes"],
    queryFn: () => agent.data!.nodes.getNodes(),
    enabled: agent.data !== undefined,
  });
}

export function useLinks() {
  const agent = useAgent();
  return useQuery<Link[]>({
    queryKey: ["links"],
    queryFn: () => agent.data!.links.list(),
    enabled: agent.data !== undefined,
  });
}

export function useKinds() {
  const agent = useAgent();
  return useQuery<Kind[]>({
    queryKey: ["kinds"],
    queryFn: () => agent.data!.kinds.list(),
    enabled: agent.data !== undefined,
    staleTime: Number.POSITIVE_INFINITY,
  });
}
