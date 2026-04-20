import type { Slot } from "@sys/agent-client";
import { useSlotEditor } from "@/lib/slots";
import type { NodeSettingsState } from "./types";

function parse(slot: Slot | undefined): Record<string, unknown> {
  const v = slot?.value;
  if (v !== null && v !== undefined && typeof v === "object" && !Array.isArray(v)) {
    return v as Record<string, unknown>;
  }
  return {};
}

function isEmpty(v: Record<string, unknown>): boolean {
  return Object.keys(v).length === 0;
}

/**
 * Manages form state for a node's `settings` slot.
 *
 * @param nodePath        - Graph path of the node (used to reset when switching nodes).
 * @param liveSettingsSlot - The live `settings` Slot from the SSE feed (or undefined).
 * @param onSave          - Async function that persists the settings (e.g. writeSlot).
 *                          Called after a 400 ms debounce.
 */
export function useNodeSettings(
  nodePath: string | undefined,
  liveSettingsSlot: Slot | undefined,
  onSave: (path: string, data: Record<string, unknown>) => Promise<void>,
): NodeSettingsState {
  const { value: formData, saveState, saveError, persist } = useSlotEditor(
    nodePath,
    liveSettingsSlot,
    parse,
    isEmpty,
    onSave,
  );

  return { formData, onChange: persist, saveState, saveError };
}
