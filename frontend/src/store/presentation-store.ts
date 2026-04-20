/**
 * PresentationStore — runtime status / color / icon / message per node.
 *
 * Kept separate from GraphStore intentionally: presentation patches arrive
 * on a high-frequency bus topic and should not trigger full-tree re-renders.
 * Components that only need structural data (sidebar, property panel) are
 * unaffected by status ticks.
 *
 * Merge rules (mirror domain-presentation::apply_patch):
 *   • fields in `patch` overwrite if `seq` is newer than current value seq.
 *   • fields in `clear` reset to `undefined` if `seq` is newer.
 *   • out-of-order patches (lower seq) are silently ignored per field.
 */

import { create } from "zustand";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type NodeStatus = "None" | "Unknown" | "Ok" | "Warning" | "Error";

export type PresentationField = "status" | "color" | "icon" | "message";

/** Sparse patch from a NodePresentationUpdate envelope. */
export interface PresentationPatch {
  status?: NodeStatus;
  color?: string;
  icon?: string;
  message?: string;
}

/** Resolved presentation for one node instance. */
export interface NodePresentation {
  status: NodeStatus;
  statusSeq: number;
  color: string | undefined;
  colorSeq: number;
  icon: string | undefined;
  iconSeq: number;
  message: string | undefined;
  messageSeq: number;
}

/** Wire envelope delivered over the SSE / NATS bus. */
export interface NodePresentationUpdate {
  node_instance_id: string;
  seq: number;
  ts: string;
  patch: PresentationPatch;
  clear?: PresentationField[];
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export interface PresentationStoreState {
  /** Keyed by node instance UUID. */
  byId: Map<string, NodePresentation>;

  applyUpdate: (update: NodePresentationUpdate) => void;
  /** Remove all state for a node (called when the node is deleted). */
  evict: (nodeId: string) => void;
  /** Reset the entire store (called on logout / tenant switch). */
  reset: () => void;
}

const DEFAULT_PRESENTATION: NodePresentation = {
  status: "None",
  statusSeq: 0,
  color: undefined,
  colorSeq: 0,
  icon: undefined,
  iconSeq: 0,
  message: undefined,
  messageSeq: 0,
};

export const usePresentationStore = create<PresentationStoreState>((set) => ({
  byId: new Map(),

  applyUpdate(update) {
    set((state) => {
      const existing = state.byId.get(update.node_instance_id) ?? { ...DEFAULT_PRESENTATION };
      const next = applyPresentationPatch(existing, update.patch, update.clear ?? [], update.seq);
      const byId = new Map(state.byId);
      byId.set(update.node_instance_id, next);
      return { byId };
    });
  },

  evict(nodeId) {
    set((state) => {
      const byId = new Map(state.byId);
      byId.delete(nodeId);
      return { byId };
    });
  },

  reset() {
    set({ byId: new Map() });
  },
}));

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/** Read the presentation for a specific node (by its UUID). */
export function useNodePresentation(nodeId: string): NodePresentation {
  return usePresentationStore((s) => s.byId.get(nodeId) ?? DEFAULT_PRESENTATION);
}

// ---------------------------------------------------------------------------
// Merge logic (mirrors domain-presentation::apply_patch)
// ---------------------------------------------------------------------------

function applyPresentationPatch(
  base: NodePresentation,
  patch: PresentationPatch,
  clear: PresentationField[],
  seq: number,
): NodePresentation {
  const next = { ...base };

  // Clear fields first (older seq still ignored).
  for (const field of clear) {
    if (field === "status" && seq > next.statusSeq) {
      next.status = "None";
      next.statusSeq = seq;
    } else if (field === "color" && seq > next.colorSeq) {
      next.color = undefined;
      next.colorSeq = seq;
    } else if (field === "icon" && seq > next.iconSeq) {
      next.icon = undefined;
      next.iconSeq = seq;
    } else if (field === "message" && seq > next.messageSeq) {
      next.message = undefined;
      next.messageSeq = seq;
    }
  }

  if (patch.status !== undefined && seq > next.statusSeq) {
    next.status = patch.status;
    next.statusSeq = seq;
  }
  if (patch.color !== undefined && seq > next.colorSeq) {
    next.color = patch.color;
    next.colorSeq = seq;
  }
  if (patch.icon !== undefined && seq > next.iconSeq) {
    next.icon = patch.icon;
    next.iconSeq = seq;
  }
  if (patch.message !== undefined && seq > next.messageSeq) {
    next.message = patch.message;
    next.messageSeq = seq;
  }

  return next;
}
