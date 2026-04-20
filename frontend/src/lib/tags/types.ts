/** Canonical tag set stored in the `config.tags` slot. */
export interface Tags {
  labels: string[];
  kv: Record<string, string>;
}

export type TagsSaveState = "idle" | "saving" | "ok" | "error";

export interface NodeTagsState {
  tags: Tags;
  saveState: TagsSaveState;
  saveError: string | null;
  addLabel: (label: string) => void;
  removeLabel: (label: string) => void;
  setKv: (key: string, value: string) => void;
  removeKv: (key: string) => void;
}
