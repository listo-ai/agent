import { create } from "zustand";

// Flow canvas state: selected node, open flow, dirty flag.
// Does not hold the graph data itself — that comes from TanStack Query / agent-client.

interface FlowState {
  /** Path of the currently open flow node, or null if none. */
  openFlowPath: string | null;
  /** Paths of selected nodes on the canvas. */
  selectedNodePaths: string[];
  /** True when the canvas has uncommitted local edits. */
  isDirty: boolean;

  setOpenFlow: (path: string | null) => void;
  setSelectedNodes: (paths: string[]) => void;
  toggleNodeSelection: (path: string) => void;
  clearSelection: () => void;
  markDirty: () => void;
  markClean: () => void;
}

export const useFlowStore = create<FlowState>()((set) => ({
  openFlowPath: null,
  selectedNodePaths: [],
  isDirty: false,

  setOpenFlow: (openFlowPath) => set({ openFlowPath, selectedNodePaths: [], isDirty: false }),
  setSelectedNodes: (selectedNodePaths) => set({ selectedNodePaths }),
  toggleNodeSelection: (path) =>
    set((s) => ({
      selectedNodePaths: s.selectedNodePaths.includes(path)
        ? s.selectedNodePaths.filter((p) => p !== path)
        : [...s.selectedNodePaths, path],
    })),
  clearSelection: () => set({ selectedNodePaths: [] }),
  markDirty: () => set({ isDirty: true }),
  markClean: () => set({ isDirty: false }),
}));
