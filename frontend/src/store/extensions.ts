import { create } from "zustand";
import type { ExtensionManifest } from "@/extensions/types";

// Extension loading state — which extensions are installed, loading, or errored.

type LoadStatus = "idle" | "loading" | "loaded" | "error";

interface ExtensionEntry {
  manifest: ExtensionManifest;
  status: LoadStatus;
  error?: string;
}

interface ExtensionsState {
  extensions: Map<string, ExtensionEntry>;

  registerManifest: (manifest: ExtensionManifest) => void;
  setStatus: (id: string, status: LoadStatus, error?: string) => void;
  getByStatus: (status: LoadStatus) => ExtensionEntry[];
}

export const useExtensionsStore = create<ExtensionsState>()((set, get) => ({
  extensions: new Map(),

  registerManifest: (manifest) =>
    set((s) => {
      const next = new Map(s.extensions);
      next.set(manifest.id, { manifest, status: "idle" });
      return { extensions: next };
    }),

  setStatus: (id, status, error) =>
    set((s) => {
      const entry = s.extensions.get(id);
      if (!entry) return s;
      const next = new Map(s.extensions);
      const updated: ExtensionEntry = error === undefined
        ? { manifest: entry.manifest, status }
        : { manifest: entry.manifest, status, error };
      next.set(id, updated);
      return { extensions: next };
    }),

  getByStatus: (status) =>
    [...get().extensions.values()].filter((e) => e.status === status),
}));
