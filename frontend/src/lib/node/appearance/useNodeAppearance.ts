import { useCallback } from "react";
import type { Slot } from "@sys/agent-client";
import { useSlotEditor } from "@/lib/slots";
import type { NodeAppearance, NodeAppearanceState } from "./types";

const EMPTY: NodeAppearance = {};

function parse(slot: Slot | undefined): NodeAppearance {
  const v = slot?.value;
  if (!v || typeof v !== "object" || Array.isArray(v)) return EMPTY;
  const obj = v as Record<string, unknown>;
  return {
    ...(typeof obj.icon === "string" ? { icon: obj.icon } : {}),
    ...(typeof obj.color === "string" ? { color: obj.color } : {}),
  };
}

function isEmpty(v: NodeAppearance): boolean {
  return !v.icon && !v.color;
}

/**
 * Manages the `config.appearance` slot value for a node.
 *
 * @param nodePath - Graph path; form resets on change.
 * @param liveSlot - The live `config.appearance` Slot from SSE.
 * @param onSave   - Async slot writer.
 */
export function useNodeAppearance(
  nodePath: string | undefined,
  liveSlot: Slot | undefined,
  onSave: (path: string, appearance: NodeAppearance) => Promise<void>,
): NodeAppearanceState {
  const { value: appearance, saveState, saveError, persist } = useSlotEditor(
    nodePath,
    liveSlot,
    parse,
    isEmpty,
    onSave,
  );

  const setIcon = useCallback(
    (icon: string) => {
      const next: NodeAppearance = { ...appearance };
      const trimmed = icon.trim();
      if (trimmed) next.icon = trimmed; else delete next.icon;
      persist(next);
    },
    [appearance, persist],
  );

  const setColor = useCallback(
    (color: string) => {
      const next: NodeAppearance = { ...appearance };
      const trimmed = color.trim();
      if (trimmed) next.color = trimmed; else delete next.color;
      persist(next);
    },
    [appearance, persist],
  );

  const clear = useCallback(
    (field: "icon" | "color") => {
      const next = { ...appearance };
      delete next[field];
      persist(next);
    },
    [appearance, persist],
  );

  return { appearance, saveState, saveError, setIcon, setColor, clear };
}

