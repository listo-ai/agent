import { useEffect, useMemo } from "react";
import type { Slot } from "@acme/agent-client";
import type { NavigateFunction } from "react-router-dom";
import { useAgent, useKinds, useLinks, useNodes } from "@/hooks/useAgent";
import { useFlowStore } from "@/store/flow";
import { isDirectChildOfFlow, toCanvasEdges, toCanvasNodes } from "./flow-model";
import { useFlowLiveData } from "./useFlowLiveData";

interface UseFlowPageDataOptions {
  pathFromUrl: string | null;
  navigate: NavigateFunction;
}

export function useFlowPageData({ pathFromUrl, navigate }: UseFlowPageDataOptions) {
  const agent = useAgent();
  const nodesQuery = useNodes();
  const linksQuery = useLinks();
  const kindsQuery = useKinds();

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

  const nodes = nodesQuery.data ?? [];
  const links = linksQuery.data ?? [];
  const kinds = kindsQuery.data ?? [];
  const kindsById = useMemo(() => new Map(kinds.map((kind) => [kind.id, kind])), [kinds]);

  useEffect(() => {
    if (!pathFromUrl) return;
    const exists = nodes.some((node) => node.path === pathFromUrl);
    if (!exists) {
      if (!nodesQuery.isPending) {
        navigate("/flows", { replace: true });
      }
      return;
    }
    if (openFlowPath !== pathFromUrl) {
      setOpenFlow(pathFromUrl);
    }
  }, [pathFromUrl, nodes, nodesQuery.isPending, openFlowPath, setOpenFlow, navigate]);

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
