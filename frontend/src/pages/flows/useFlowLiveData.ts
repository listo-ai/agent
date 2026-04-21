/**
 * Derives a `Record<nodePath, LiveNodeState>` directly from the GraphStore's
 * node cache.
 *
 * The GraphStore already maintains a single SSE subscription and applies
 * `slot_changed` / `lifecycle_transition` events to its `nodes` Map in real
 * time. Reading from that Map here — via a `useStore` selector — eliminates
 * the need for a second SSE connection that the previous implementation
 * opened independently.
 */
import { useMemo } from "react";
import { create, useStore } from "zustand";
import type { NodeSnapshot } from "@listo/agent-client";
import type { GraphStore } from "@/store/graph-hooks";
import { slotMap, type LiveNodeState } from "./flow-model";

/** Stable empty store used when GraphStore hasn't connected yet. */
const emptyLiveStore = create(() => ({
  nodes: new Map<string, NodeSnapshot>(),
}));

export function useFlowLiveData(
  graphStore: GraphStore | null,
): Record<string, LiveNodeState> {
  const activeStore = graphStore ?? emptyLiveStore;
  const nodeMap = useStore(activeStore, (s) => s.nodes);

  return useMemo(() => {
    const result: Record<string, LiveNodeState> = {};
    for (const node of nodeMap.values()) {
      result[node.path] = {
        lifecycle: node.lifecycle as LiveNodeState["lifecycle"],
        slots: slotMap(node.slots),
        touchedAt: undefined,
      };
    }
    return result;
  }, [nodeMap]);
}

