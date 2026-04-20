import { create } from "zustand";
import type { BlockManifest } from "@/blocks/types";

// Block loading state — which blocks are installed, loading, or errored.

type LoadStatus = "idle" | "loading" | "loaded" | "error";

interface BlockEntry {
  manifest: BlockManifest;
  status: LoadStatus;
  error?: string;
}

interface ExtensionsState {
  blocks: Map<string, BlockEntry>;

  registerManifest: (manifest: BlockManifest) => void;
  setStatus: (id: string, status: LoadStatus, error?: string) => void;
  getByStatus: (status: LoadStatus) => BlockEntry[];
}

export const useBlocksStore = create<ExtensionsState>()((set, get) => ({
  blocks: new Map(),

  registerManifest: (manifest) =>
    set((s) => {
      const next = new Map(s.blocks);
      next.set(manifest.id, { manifest, status: "idle" });
      return { blocks: next };
    }),

  setStatus: (id, status, error) =>
    set((s) => {
      const entry = s.blocks.get(id);
      if (!entry) return s;
      const next = new Map(s.blocks);
      const updated: BlockEntry = error === undefined
        ? { manifest: entry.manifest, status }
        : { manifest: entry.manifest, status, error };
      next.set(id, updated);
      return { blocks: next };
    }),

  getByStatus: (status) =>
    [...get().blocks.values()].filter((e) => e.status === status),
}));
