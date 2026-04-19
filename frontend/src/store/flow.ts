import { create } from "zustand";

// Flow canvas state: selected node, open flow, dirty flag.
// Does not hold the graph data itself — that comes from TanStack Query / agent-client.

interface FlowState {
  /** Path of the currently open flow node, or null if none. */
  openFlowPath: string | null;
  /** Paths of selected nodes on the canvas. */
  selectedNodePaths: string[];
  /** IDs of selected links on the canvas. */
  selectedEdgeIds: string[];
  /** Canvas presentation toggles. */
  showGrid: boolean;
  showMiniMap: boolean;

  setOpenFlow: (path: string | null) => void;
  setSelectedNodes: (paths: string[]) => void;
  setSelectedEdges: (ids: string[]) => void;
  toggleNodeSelection: (path: string) => void;
  clearSelection: () => void;
  toggleGrid: () => void;
  toggleMiniMap: () => void;
}

export const useFlowStore = create<FlowState>()((set) => ({
  openFlowPath: null,
  selectedNodePaths: [],
  selectedEdgeIds: [],
  showGrid: true,
  showMiniMap: true,

  setOpenFlow: (openFlowPath) =>
    set({ openFlowPath, selectedNodePaths: [], selectedEdgeIds: [] }),
  setSelectedNodes: (selectedNodePaths) => set({ selectedNodePaths }),
  setSelectedEdges: (selectedEdgeIds) => set({ selectedEdgeIds }),
  toggleNodeSelection: (path) =>
    set((s) => ({
      selectedNodePaths: s.selectedNodePaths.includes(path)
        ? s.selectedNodePaths.filter((p) => p !== path)
        : [...s.selectedNodePaths, path],
    })),
  clearSelection: () => set({ selectedNodePaths: [], selectedEdgeIds: [] }),
  toggleGrid: () => set((s) => ({ showGrid: !s.showGrid })),
  toggleMiniMap: () => set((s) => ({ showMiniMap: !s.showMiniMap })),
}));
