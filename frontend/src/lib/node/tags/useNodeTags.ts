import { useCallback } from "react";
import type { Slot } from "@listo/agent-client";
import { useSlotEditor } from "@/lib/slots";
import type { NodeTagsState, Tags } from "./types";

const EMPTY_TAGS: Tags = { labels: [], kv: {} };

function parse(slot: Slot | undefined): Tags {
  const v = slot?.value;
  if (v === null || v === undefined || typeof v !== "object" || Array.isArray(v)) {
    return EMPTY_TAGS;
  }
  const obj = v as Record<string, unknown>;
  const labels = Array.isArray(obj.labels)
    ? (obj.labels as unknown[]).filter((l): l is string => typeof l === "string")
    : [];
  const kv: Record<string, string> = {};
  if (obj.kv && typeof obj.kv === "object" && !Array.isArray(obj.kv)) {
    for (const [k, val] of Object.entries(obj.kv as Record<string, unknown>)) {
      if (typeof val === "string") kv[k] = val;
    }
  }
  return { labels, kv };
}

function isEmpty(v: Tags): boolean {
  return v.labels.length === 0 && Object.keys(v.kv).length === 0;
}

/**
 * Manages `config.tags` slot state for a node.
 *
 * @param nodePath       - Graph path of the node; resets state on change.
 * @param liveTagsSlot   - The live `config.tags` Slot from the SSE feed.
 * @param onSave         - Async writer: `(path, tags) => Promise<void>`.
 */
export function useNodeTags(
  nodePath: string | undefined,
  liveTagsSlot: Slot | undefined,
  onSave: (path: string, tags: Tags) => Promise<void>,
): NodeTagsState {
  const { value: tags, saveState, saveError, persist } = useSlotEditor(
    nodePath,
    liveTagsSlot,
    parse,
    isEmpty,
    onSave,
  );

  const addLabel = useCallback(
    (raw: string) => {
      const label = raw.trim().toLowerCase();
      if (!label || tags.labels.includes(label)) return;
      persist({ ...tags, labels: [...tags.labels, label] });
    },
    [tags, persist],
  );

  const removeLabel = useCallback(
    (label: string) => {
      persist({ ...tags, labels: tags.labels.filter((l) => l !== label) });
    },
    [tags, persist],
  );

  const setKv = useCallback(
    (key: string, value: string) => {
      const k = key.trim().toLowerCase();
      const v = value.trim();
      if (!k || !v) return;
      persist({ ...tags, kv: { ...tags.kv, [k]: v } });
    },
    [tags, persist],
  );

  const removeKv = useCallback(
    (key: string) => {
      const kv = { ...tags.kv };
      delete kv[key];
      persist({ ...tags, kv });
    },
    [tags, persist],
  );

  return { tags, saveState, saveError, addLabel, removeLabel, setKv, removeKv };
}

