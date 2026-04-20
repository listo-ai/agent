/**
 * Generic slot-editor hook.
 *
 * Encapsulates the identical state machine shared by every "edit-a-slot" hook
 * (node-settings, tags, appearance, …):
 *
 *  1. Reset local state when `nodePath` changes.
 *  2. Seed from the live SSE slot once it arrives if the local state is still at
 *     its initial/empty value (i.e., don't overwrite a user's in-flight edit).
 *  3. Debounce writes and track idle / saving / ok / error.
 *
 * Domain-specific hooks (`useNodeTags`, `useNodeSettings`, …) are thin wrappers
 * around this one: they supply `parse` + `isEmpty` and then derive their own
 * action helpers (`addLabel`, `setIcon`, …) from the returned `persist`.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { Slot } from "@sys/agent-client";

export type SaveState = "idle" | "saving" | "ok" | "error";

export interface SlotEditorState<T> {
  value: T;
  saveState: SaveState;
  saveError: string | null;
  /** Immediately updates local state and schedules a debounced write. */
  persist: (next: T) => void;
}

/**
 * @param nodePath   - Graph path of the node; resets all state on change.
 * @param liveSlot   - The live Slot from the SSE feed (or undefined).
 * @param parse      - Converts a raw Slot to the domain value `T`.
 * @param isEmpty    - Returns true when `T` is still at its "pristine" initial
 *                     state so the seed effect knows it's safe to overwrite.
 * @param onSave     - Async writer called after the debounce.
 * @param debounceMs - Debounce delay in ms (default 400).
 */
export function useSlotEditor<T>(
  nodePath: string | undefined,
  liveSlot: Slot | undefined,
  parse: (slot: Slot | undefined) => T,
  isEmpty: (v: T) => boolean,
  onSave: (path: string, value: T) => Promise<void>,
  debounceMs = 400,
): SlotEditorState<T> {
  const initial = () => parse(liveSlot);
  const [value, setValue] = useState<T>(initial);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  // Reset when the selected node changes.
  useEffect(() => {
    window.clearTimeout(saveTimer.current);
    setValue(parse(liveSlot));
    setSaveState("idle");
    setSaveError(null);
    // Intentionally not including liveSlot — we only want this on path change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodePath]);

  // Seed from the live slot once it arrives if local state is still pristine.
  const generation = liveSlot?.generation;
  useEffect(() => {
    setValue((prev) => (isEmpty(prev) ? parse(liveSlot) : prev));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [generation]);

  const persist = useCallback(
    (next: T) => {
      if (!nodePath) return;
      setValue(next);
      window.clearTimeout(saveTimer.current);
      setSaveState("saving");
      saveTimer.current = window.setTimeout(() => {
        onSave(nodePath, next)
          .then(() => {
            setSaveState("ok");
            setSaveError(null);
            saveTimer.current = window.setTimeout(
              () => setSaveState("idle"),
              2000,
            );
          })
          .catch((err: unknown) => {
            setSaveState("error");
            setSaveError(err instanceof Error ? err.message : String(err));
          });
      }, debounceMs);
    },
    [nodePath, onSave, debounceMs],
  );

  return { value, saveState, saveError, persist };
}
