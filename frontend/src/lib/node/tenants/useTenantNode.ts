import { useCallback } from "react";
import type { Slot } from "@sys/agent-client";
import { useSlotEditor } from "@/lib/slots";
import type { TenantNodeState } from "./types";

// ---------------------------------------------------------------------------
// Slot parsers
// ---------------------------------------------------------------------------

function parse(slot: Slot | undefined): string {
  const v = slot?.value;
  return typeof v === "string" ? v : "";
}

function isEmpty(v: string): boolean {
  return v.trim() === "";
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Manages the `name` slot for a `sys.auth.tenant` node.
 *
 * Tags are handled separately with `useNodeTags` — the `config.tags` slot
 * has the same shape on tenant nodes as on any other node.
 *
 * @param nodePath      - Graph path of the tenant node; resets state on change.
 * @param liveNameSlot  - The live `name` Slot from the SSE feed.
 * @param onSave        - Async writer: `(path, name) => Promise<void>`.
 */
export function useTenantNode(
  nodePath: string | undefined,
  liveNameSlot: Slot | undefined,
  onSave: (path: string, name: string) => Promise<void>,
): TenantNodeState {
  const { value: name, saveState, saveError, persist } = useSlotEditor(
    nodePath,
    liveNameSlot,
    parse,
    isEmpty,
    onSave,
  );

  const setName = useCallback(
    (v: string) => {
      persist(v);
    },
    [persist],
  );

  return { name, saveState, saveError, setName };
}
