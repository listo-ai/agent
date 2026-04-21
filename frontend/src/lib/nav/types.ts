// ---------------------------------------------------------------------------
// NavNode — frontend domain type for a ui.nav tree node.
// Mapped from UiNavNode (clients/ts/src/schemas/ui.ts) so UI never imports
// client schema types directly.
// ---------------------------------------------------------------------------

export interface NavNode {
  id: string;
  /** Display title — falls back to id when null. */
  title: string | null;
  /** Graph path of the associated ui.page (null = folder-only entry). */
  path: string | null;
  /** Lucide icon name (string) or null. */
  icon: string | null;
  /** Sort order within siblings. */
  order: number | null;
  /** Frame alias contributed to the context stack on entry. */
  frameAlias: string | null;
  children: NavNode[];
}

// ---------------------------------------------------------------------------
// Hook state
// ---------------------------------------------------------------------------

export type NavTreeStatus = "loading" | "error" | "ready" | "unconfigured";

export interface NavTreeState {
  status: NavTreeStatus;
  root: NavNode | null;
  errorDetail: string | null;
  refetch: () => void;
}
