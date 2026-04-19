/**
 * React hooks for reading from a GraphStore.
 *
 * All hooks accept a `store` argument so they can be used with any store
 * instance (simplifies testing and multi-agent scenarios).
 *
 * Example
 * -------
 *   const store = useGraphStore();
 *   const children = useNodeChildren(store, "/flows");
 */

import { useStore } from "zustand";
import type { NodeSnapshot, Link } from "@acme/agent-client";
import type { GraphStore } from "./graph-store";
export { createGraphStore } from "./graph-store";
export type { GraphStore };

// ---------------------------------------------------------------------------
// Context (set this up once near the app root)
// ---------------------------------------------------------------------------

import { createContext, useContext } from "react";

export const GraphStoreContext = createContext<GraphStore | null>(null);

/** Returns the nearest GraphStore from context. Throws if missing. */
export function useGraphStore(): GraphStore {
  const store = useContext(GraphStoreContext);
  if (!store) {
    throw new Error(
      "useGraphStore: no GraphStore found in context. " +
        "Wrap your tree with <GraphStoreContext.Provider value={store}>.",
    );
  }
  return store;
}

/**
 * Returns the GraphStore from context, or null if it hasn't finished
 * connecting yet. Use this in top-level pages that boot alongside the
 * GraphStoreProvider and may render before the agent handshake is done.
 */
export function useGraphStoreOptional(): GraphStore | null {
  return useContext(GraphStoreContext);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Stable empty arrays / maps so selectors never trigger needless renders. */
const EMPTY_NODES: NodeSnapshot[] = [];
const EMPTY_LINKS: Link[] = [];

// ---------------------------------------------------------------------------
// Sidebar hooks
// ---------------------------------------------------------------------------

/**
 * Returns cached children for `parentPath`, ordered by path.
 * Subscribes to the store so the component updates when children are added /
 * removed / renamed.
 */
export function useNodeChildren(
  store: GraphStore,
  parentPath: string,
): NodeSnapshot[] {
  return useStore(store, (s) => {
    const children: NodeSnapshot[] = [];
    for (const n of s.nodes.values()) {
      if (n.parent_path === parentPath) children.push(n);
    }
    if (children.length === 0) return EMPTY_NODES;
    return children.sort((a, b) => a.path.localeCompare(b.path));
  });
}

/** Returns true when `path` is in the expanded set. */
export function useExpanded(store: GraphStore, path: string): boolean {
  return useStore(store, (s) => s.expanded.has(path));
}

/** Returns `expand` and `collapse` actions bound to `path`. */
export function useExpandActions(
  store: GraphStore,
  path: string,
): { expand: () => void; collapse: () => void } {
  const expand = useStore(store, (s) => s.expand);
  const collapse = useStore(store, (s) => s.collapse);
  return {
    expand: () => expand(path),
    collapse: () => collapse(path),
  };
}

/** Returns the cached NodeSnapshot for `path`, or undefined. */
export function useNode(
  store: GraphStore,
  path: string,
): NodeSnapshot | undefined {
  return useStore(store, (s) => s.nodes.get(path));
}

// ---------------------------------------------------------------------------
// Canvas hooks
// ---------------------------------------------------------------------------

/** Returns all nodes whose path starts with `flowPath + "/"`. */
export function useFlowSubtree(
  store: GraphStore,
  flowPath: string,
): NodeSnapshot[] {
  const prefix = flowPath.endsWith("/") ? flowPath : `${flowPath}/`;
  return useStore(store, (s) => {
    const nodes: NodeSnapshot[] = [];
    for (const n of s.nodes.values()) {
      if (n.path === flowPath || n.path.startsWith(prefix)) nodes.push(n);
    }
    return nodes.length === 0 ? EMPTY_NODES : nodes;
  });
}

/** Returns all cached links. */
export function useLinks(store: GraphStore): Link[] {
  return useStore(store, (s) => {
    const all = [...s.links.values()];
    return all.length === 0 ? EMPTY_LINKS : all;
  });
}

/** Returns the currently open flow path (canvas). */
export function useOpenFlow(store: GraphStore): string | null {
  return useStore(store, (s) => s.openFlow);
}

// ---------------------------------------------------------------------------
// Property panel hooks
// ---------------------------------------------------------------------------

/** Returns the currently selected node path. */
export function useSelection(store: GraphStore): string | null {
  return useStore(store, (s) => s.selection);
}

/** Returns the selected NodeSnapshot or undefined. */
export function useSelectedNode(store: GraphStore): NodeSnapshot | undefined {
  return useStore(store, (s) =>
    s.selection ? s.nodes.get(s.selection) : undefined,
  );
}

// ---------------------------------------------------------------------------
// Slot write hook
// ---------------------------------------------------------------------------

export interface UseSlotWriteResult {
  /** Current slot value from the cache (includes optimistic writes). */
  value: unknown;
  /** True while an optimistic write is in flight. */
  isPending: boolean;
  /** Non-null if a conflict was detected on this slot. */
  conflict: unknown;
  /** Write a new value to this slot. */
  write: (value: unknown) => Promise<void>;
}

/**
 * Returns the current value of `slot` on `path` plus a `write()` action
 * that performs an optimistic update.
 */
export function useSlotWrite(
  store: GraphStore,
  path: string,
  slot: string,
): UseSlotWriteResult {
  const key = `${path}::${slot}`;

  const value = useStore(store, (s) => {
    const n = s.nodes.get(path);
    return n?.slots.find((sl) => sl.name === slot)?.value ?? null;
  });

  const isPending = useStore(store, (s) => s.pending.has(key));
  const conflict = useStore(store, (s) => s.conflicts.get(key) ?? null);
  const writeSlot = useStore(store, (s) => s.writeSlot);

  return {
    value,
    isPending,
    conflict,
    write: (v) => writeSlot(path, slot, v),
  };
}

// ---------------------------------------------------------------------------
// Loading / status hooks
// ---------------------------------------------------------------------------

/** True while children for `path` are being fetched. */
export function useIsLoading(store: GraphStore, path: string): boolean {
  return useStore(store, (s) => s.loadingPaths.has(path));
}

/** The last sequence number seen from the SSE stream. */
export function useLastSeq(store: GraphStore): number {
  return useStore(store, (s) => s.lastSeq);
}
