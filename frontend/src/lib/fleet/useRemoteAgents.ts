/**
 * useRemoteAgents — reads `sys.fleet.remote-agent` nodes from the graph store
 * and maps each one to a `RemoteAgentNode` domain value.
 *
 * Pure logic — no UI, no routing knowledge. Components import this and pass
 * the result into a presentational component.
 *
 * Returns an empty array when:
 *   - No `GraphStore` is present in context (before the agent handshake).
 *   - No `sys.fleet.remote-agent` nodes exist in the graph.
 */

import { useStore, create } from "zustand";
import { FleetScope } from "@listo/agent-client";
import type { Link, NodeSnapshot } from "@listo/agent-client";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import type { RemoteAgentNode, RemoteAgentConnection } from "./types";

// Stable empty store used when the real GraphStore is not yet ready.
// Module-level so the same reference is reused across renders — prevents
// the `useStore` selector from being called with a different store instance
// on every render.
const emptyStore = create(() => ({
  nodes: new Map<string, NodeSnapshot>(),
  links: new Map<string, Link>(),
}));

const KIND = "sys.fleet.remote-agent";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function slotValue(node: NodeSnapshot, name: string): string | null {
  const slot = node.slots.find((s) => s.name === name);
  const v = slot?.value;
  return typeof v === "string" && v.length > 0 ? v : null;
}

function toConnection(raw: string | null): RemoteAgentConnection {
  switch (raw) {
    case "connected":
    case "reconnecting":
    case "disconnected":
      return raw;
    default:
      return "unknown";
  }
}

function toRemoteAgentNode(node: NodeSnapshot): RemoteAgentNode | null {
  const tenant = slotValue(node, "tenant");
  const agentId = slotValue(node, "agent_id");
  if (!tenant || !agentId) {
    // Slots not yet written or node is partially constructed — skip.
    return null;
  }
  const displayName = slotValue(node, "display_name") ?? agentId;
  const connection = toConnection(slotValue(node, "connection"));
  const lastSeen = slotValue(node, "last_seen");
  const scope = FleetScope.remote(tenant, agentId);

  return {
    id: node.id,
    path: node.path,
    displayName,
    tenant,
    agentId,
    connection,
    lastSeen,
    scope,
    // URL is the source of truth: navigating here pushes the scope.
    scopeUrl: `/scope/${encodeURIComponent(tenant)}/${encodeURIComponent(agentId)}`,
  };
}

// ---------------------------------------------------------------------------
// Sentinel for when graphStore is absent
// ---------------------------------------------------------------------------

const EMPTY: RemoteAgentNode[] = [];

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Returns every `sys.fleet.remote-agent` node visible in the local graph,
 * mapped to `RemoteAgentNode` values.
 *
 * Reactively re-evaluates whenever the graph store updates so connection
 * status badges update in real-time.
 */
export function useRemoteAgents(): RemoteAgentNode[] {
  const store = useGraphStoreOptional();
  const active = store ?? emptyStore;

  return useStore(active, (s) => {
    const result: RemoteAgentNode[] = [];
    for (const node of s.nodes.values()) {
      if (node.kind !== KIND) continue;
      const mapped = toRemoteAgentNode(node);
      if (mapped) result.push(mapped);
    }
    result.sort((a, b) => a.displayName.localeCompare(b.displayName));
    return result.length > 0 ? result : EMPTY;
  });
}
