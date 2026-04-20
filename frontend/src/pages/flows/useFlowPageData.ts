import { useEffect, useMemo } from "react";
import { create, useStore } from "zustand";
import type { Link, NodeSnapshot, Slot } from "@sys/agent-client";
import type { NavigateFunction } from "react-router-dom";
import { useAgent, useKinds } from "@/hooks/useAgent";
import { useFlowStore } from "@/store/flow";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { isDirectChildOfFlow, toCanvasEdges, toCanvasNodes } from "./flow-model";
import { useFlowLiveData } from "./useFlowLiveData";

/** Stable empty store used before GraphStore connects. */
const emptyFlowStore = create(() => ({
  nodes: new Map<string, NodeSnapshot>(),
  links: new Map<string, Link>(),
  loadingPaths: new Set<string>(),
}));

interface UseFlowPageDataOptions {
  pathFromUrl: string | null;
  navigate: NavigateFunction;
}

export function useFlowPageData({ pathFromUrl, navigate }: UseFlowPageDataOptions) {
  const agent = useAgent();
  const kindsQuery = useKinds();
  const graphStore = useGraphStoreOptional();

  const openFlowPath = useFlowStore((state) => state.openFlowPath);
  const selectedNodePaths = useFlowStore((state) => state.selectedNodePaths);
  const selectedEdgeIds = useFlowStore((state) => state.selectedEdgeIds);
  const showGrid = useFlowStore((state) => state.showGrid);
  const showMiniMap = useFlowStore((state) => state.showMiniMap);
  const setOpenFlow = useFlowStore((state) => state.setOpenFlow);
  const setSelectedNodes = useFlowStore((state) => state.setSelectedNodes);
  const setSelectedEdges = useFlowStore((state) => state.setSelectedEdges);
  const toggleGrid = useFlowStore((state) => state.toggleGrid);
  const toggleMiniMap = useFlowStore((state) => state.toggleMiniMap);

  // ---- Reactive node / link data from GraphStore ----
  const activeStore = graphStore ?? emptyFlowStore;

  const nodeMap = useStore(activeStore, (s) => s.nodes);
  const linkMap = useStore(activeStore, (s) => s.links);
  const loadingPaths = useStore(activeStore, (s) => s.loadingPaths);

  const nodes = useMemo(() => [...nodeMap.values()], [nodeMap]);
  const links = useMemo(() => [...linkMap.values()], [linkMap]);

  // Sync the URL's flow path into the flow store immediately so the
  // GraphStore starts fetching before we check whether the node exists.
  useEffect(() => {
    if (!pathFromUrl) return;
    if (openFlowPath !== pathFromUrl) {
      setOpenFlow(pathFromUrl);
    }
  }, [pathFromUrl, openFlowPath, setOpenFlow]);

  // Tell the GraphStore which flow path is open so it auto-fetches the subtree.
  useEffect(() => {
    if (!graphStore || !openFlowPath) return;
    graphStore.getState().setOpenFlow(openFlowPath);
  }, [graphStore, openFlowPath]);

  // Consider loading when:
  //  - GraphStore hasn't connected yet, OR
  //  - A flow is open and its subtree is still being fetched, OR
  //  - We have a URL path but nodes haven't arrived yet (initial fetch)
  const isLoading =
    !graphStore ||
    (!!openFlowPath && loadingPaths.has(openFlowPath)) ||
    (!!pathFromUrl && nodes.length === 0);

  const kinds = kindsQuery.data ?? [];
  const kindsById = useMemo(() => new Map(kinds.map((kind) => [kind.id, kind])), [kinds]);

  useEffect(() => {
    if (!pathFromUrl) return;
    // Still loading — don't redirect yet.
    if (isLoading) return;
    const exists = nodes.some((node) => node.path === pathFromUrl);
    if (!exists) {
      navigate("/flows", { replace: true });
      return;
    }
  }, [pathFromUrl, nodes, isLoading, navigate]);

  const visibleNodes = useMemo(
    () => (openFlowPath ? nodes.filter((node) => isDirectChildOfFlow(node, openFlowPath)) : []),
    [nodes, openFlowPath],
  );
  const visibleNodeIds = useMemo(() => new Set(visibleNodes.map((node) => node.id)), [visibleNodes]);
  const visibleLinks = useMemo(
    () =>
      links.filter(
        (link) => visibleNodeIds.has(link.source.node_id) && visibleNodeIds.has(link.target.node_id),
      ),
    [links, visibleNodeIds],
  );
  const visibleNodeMap = useMemo(() => new Map(visibleNodes.map((node) => [node.id, node])), [visibleNodes]);
  const visibleNodeList = useMemo(
    () => toCanvasNodes(visibleNodes, kindsById),
    [visibleNodes, kindsById],
  );
  const edgeList = useMemo(() => toCanvasEdges(visibleLinks, visibleNodeMap), [visibleLinks, visibleNodeMap]);

  const liveByPath = useFlowLiveData(agent.data, nodes);
  const selectedNode = useMemo(
    () => nodes.find((node) => selectedNodePaths.includes(node.path)),
    [nodes, selectedNodePaths],
  );
  const selectedKind = selectedNode ? kindsById.get(selectedNode.kind) : undefined;
  const selectedLive: Record<string, Slot> = selectedNode ? (liveByPath[selectedNode.path]?.slots ?? {}) : {};

  // Synthetic query-compatible objects so FlowsPage loading/error guards still work.
  const nodesQuery = { isPending: isLoading, isError: false, error: null, data: nodes } as const;
  const linksQuery = { isPending: isLoading, isError: false, error: null, data: links } as const;

  return {
    agent,
    nodesQuery,
    linksQuery,
    kindsQuery,
    kinds,
    kindsById,
    openFlowPath,
    selectedNodePaths,
    selectedEdgeIds,
    showGrid,
    showMiniMap,
    setSelectedNodes,
    setSelectedEdges,
    toggleGrid,
    toggleMiniMap,
    visibleNodes,
    visibleNodeMap,
    visibleNodeList,
    edgeList,
    liveByPath,
    selectedNode,
    selectedKind,
    selectedLive,
  };
}
