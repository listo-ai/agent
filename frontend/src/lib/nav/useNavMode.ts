import { useUiStore } from "@/store/ui";
import type { NavMode } from "@/store/ui";

/**
 * Which sidebar mode is active.
 *
 * - `"admin"` — full Studio UI (flows tree, pages, blocks, settings).
 * - `"user"`  — user-facing nav driven by `ui.nav` graph nodes.
 */
export type { NavMode }

export interface NavModeState {
  navMode: NavMode;
  /** The root `ui.nav` node id for user mode (null until configured). */
  userNavRootId: string | null;
  setNavMode: (mode: NavMode) => void;
  setUserNavRootId: (id: string | null) => void;
}

/** Reads + writes nav mode from the persisted UI store. */
export function useNavMode(): NavModeState {
  const navMode = useUiStore((s) => s.navMode);
  const userNavRootId = useUiStore((s) => s.userNavRootId);
  const setNavMode = useUiStore((s) => s.setNavMode);
  const setUserNavRootId = useUiStore((s) => s.setUserNavRootId);
  return { navMode, userNavRootId, setNavMode, setUserNavRootId };
}
