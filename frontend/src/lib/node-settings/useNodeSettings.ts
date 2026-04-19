import { useEffect, useRef, useState } from "react";
import type { Slot } from "@sys/agent-client";
import type { NodeSettingsState } from "./types";

/**
 * Manages form state for a node's `settings` slot.
 *
 * @param nodePath  - Graph path of the node (used to reset when switching nodes).
 * @param liveSettingsSlot - The live `settings` Slot from the SSE feed (or undefined).
 * @param onSave    - Async function that persists the settings (e.g. writeSlot).
 *                    Called after a 400 ms debounce.
 */
export function useNodeSettings(
  nodePath: string | undefined,
  liveSettingsSlot: Slot | undefined,
  onSave: (path: string, data: Record<string, unknown>) => Promise<void>,
): NodeSettingsState {
  const [formData, setFormData] = useState<Record<string, unknown>>({});
  const [saveState, setSaveState] = useState<NodeSettingsState["saveState"]>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  // Reset when the selected node changes.
  useEffect(() => {
    window.clearTimeout(saveTimer.current);
    const v = liveSettingsSlot?.value;
    if (v !== null && v !== undefined && typeof v === "object" && !Array.isArray(v)) {
      setFormData(v as Record<string, unknown>);
    } else {
      setFormData({});
    }
    setSaveState("idle");
    setSaveError(null);
    // Intentionally not including liveSettingsSlot — we only want this on path change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodePath]);

  // Seed from live slot once it arrives (generation changes) if form is still pristine.
  const generation = liveSettingsSlot?.generation;
  useEffect(() => {
    const v = liveSettingsSlot?.value;
    if (v !== null && v !== undefined && typeof v === "object" && !Array.isArray(v)) {
      setFormData((prev) =>
        Object.keys(prev).length === 0 ? (v as Record<string, unknown>) : prev,
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [generation]);

  const onChange = (data: Record<string, unknown>) => {
    if (!nodePath) return;
    setFormData(data);
    window.clearTimeout(saveTimer.current);
    setSaveState("saving");
    saveTimer.current = window.setTimeout(() => {
      onSave(nodePath, data)
        .then(() => {
          setSaveState("ok");
          setSaveError(null);
          saveTimer.current = window.setTimeout(() => setSaveState("idle"), 2000);
        })
        .catch((err: unknown) => {
          setSaveState("error");
          setSaveError(err instanceof Error ? err.message : String(err));
        });
    }, 400);
  };

  return { formData, onChange, saveState, saveError };
}
