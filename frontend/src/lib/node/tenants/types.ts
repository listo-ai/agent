import type { Tags } from "../tags";

// ---------------------------------------------------------------------------
// Domain type
// ---------------------------------------------------------------------------

/** Represents a `sys.auth.tenant` node read from the graph. */
export interface TenantNode {
  id: string;
  path: string;
  /** `name` slot. */
  name: string | null;
  tags: Tags;
  /** Count of direct child user nodes (passed in by caller from graph data). */
  memberCount: number;
}

// ---------------------------------------------------------------------------
// Slot-level state (for editing a single tenant's name slot)
// ---------------------------------------------------------------------------

export type TenantSaveState = "idle" | "saving" | "ok" | "error";

export interface TenantNodeState {
  /** Current name value. */
  name: string;
  saveState: TenantSaveState;
  saveError: string | null;
  /** Update the name; persisted after a 400 ms debounce via `onSave`. */
  setName: (v: string) => void;
}
