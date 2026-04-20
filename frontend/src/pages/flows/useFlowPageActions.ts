import { useCallback, useEffect, useRef, useState } from "react";
import type { Kind, NodeSnapshot } from "@sys/agent-client";
import { useMutation } from "@tanstack/react-query";
import type {
  Connection,
  Edge,
  Node,
  OnSelectionChangeParams,
  ReactFlowInstance,
} from "@xyflow/react";
import type { NavigateFunction } from "react-router-dom";
import { queryClient } from "@/providers/query";
import { useAgent } from "@/hooks/useAgent";
import { useInvalidateGraph } from "@/lib/node";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { formatError } from "@/lib/utils";
import {
  autoLayout,
  nextNodeName,
  POSITION_SLOT,
  type FlowNodeData,
} from "./flow-model";

interface UseFlowPageActionsOptions {
  navigate: NavigateFunction;
  openFlowPath: string | null;
  kindsById: Map<string, Kind>;
  visibleNodes: NodeSnapshot[];
  visibleNodeMap: Map<string, NodeSnapshot>;
  visibleNodeList: Node<FlowNodeData>[];
  edgeList: Edge[];
  selectedNodePaths: string[];
  selectedEdgeIds: string[];
  setSelectedNodes: (paths: string[]) => void;
  setSelectedEdges: (ids: string[]) => void;
}

export function useFlowPageActions({
  navigate,
  openFlowPath,
  kindsById,
  visibleNodes,
  visibleNodeMap,
  visibleNodeList,
  edgeList,
  selectedNodePaths,
  selectedEdgeIds,
  setSelectedNodes,
  setSelectedEdges,
}: UseFlowPageActionsOptions) {
  const agent = useAgent().data;
  const graphStore = useGraphStoreOptional();
  const flowRef = useRef<ReactFlowInstance<Node<FlowNodeData>, Edge> | null>(null);
  const moveTimers = useRef(new Map<string, number>());
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [addChildPath, setAddChildPath] = useState<string | null>(null);

  const invalidateGraphQueries = useInvalidateGraph();

  const createNodeMutation = useMutation({
    mutationFn: async ({ kind, position }: { kind: Kind; position: { x: number; y: number } }) => {
      if (!openFlowPath || !agent) return;
      const name = nextNodeName(kind, visibleNodes.map((node) => node.path));
      const created = await agent.nodes.createNode({
        parent: openFlowPath,
        kind: kind.id,
        name,
      });
      // Immediately inject an optimistic snapshot with the drop position so
      // the canvas renders the node at the correct location right away —
      // before the SSE node_created → batch-fetch round-trip completes.
      graphStore?.getState()._mergeNodes([{
        id: created.id,
        kind: kind.id,
        path: created.path,
        parent_path: openFlowPath,
        parent_id: null,
        has_children: false,
        lifecycle: "created",
        slots: [{ name: POSITION_SLOT, value: position, generation: 0 }],
      }]);
      // Use graphStore.writeSlot so the position is protected by the pending
      // map and won't be clobbered if the SSE batch-fetch arrives before the
      // server processes the slot write.
      await (graphStore
        ? graphStore.getState().writeSlot(created.path, POSITION_SLOT, position)
        : agent.slots.writeSlot(created.path, POSITION_SLOT, position));
    },
    onSuccess: async () => {
      await invalidateGraphQueries();
      setErrorMessage(null);
    },
    onError: (error) => setErrorMessage(formatError(error)),
  });

  const createNode = useCallback(
    (kind: Kind, position?: { x: number; y: number }) => {
      if (!openFlowPath || !flowRef.current) {
        return;
      }
      const flowPosition = position
        ? flowRef.current.screenToFlowPosition(position)
        : { x: 120 + visibleNodes.length * 20, y: 120 + visibleNodes.length * 14 };
      createNodeMutation.mutate({ kind, position: flowPosition });
    },
    [createNodeMutation, openFlowPath, visibleNodes.length],
  );

  const persistNodePosition = useCallback(
    (node: Node<FlowNodeData>) => {
      if (!agent) return;
      const path = node.data.snapshot.path;
      const previous = moveTimers.current.get(path);
      if (previous) {
        window.clearTimeout(previous);
      }
      const timer = window.setTimeout(async () => {
        try {
          await agent.slots.writeSlot(path, "position", node.position);
          await queryClient.invalidateQueries({ queryKey: ["nodes"] });
          setErrorMessage(null);
        } catch (error) {
          setErrorMessage(formatError(error));
        } finally {
          moveTimers.current.delete(path);
        }
      }, 220);
      moveTimers.current.set(path, timer);
    },
    [agent],
  );

  const handleOpenNode = useCallback(
    (nodePath: string) => {
      navigate(`/flows/edit${nodePath}`);
    },
    [navigate],
  );

  const handleAddChildNode = useCallback((nodePath: string) => {
    setAddChildPath(nodePath);
  }, []);

  const handleConnect = useCallback(
    async (connection: Connection) => {
      if (!agent || !connection.source || !connection.target || !connection.sourceHandle || !connection.targetHandle) {
        return;
      }
      const source = visibleNodeMap.get(connection.source);
      const target = visibleNodeMap.get(connection.target);
      if (!source || !target) {
        return;
      }

      try {
        await agent.links.create(
          { path: source.path, slot: connection.sourceHandle },
          { path: target.path, slot: connection.targetHandle },
        );
        await queryClient.invalidateQueries({ queryKey: ["links"] });
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent, visibleNodeMap],
  );

  const deleteNodes = useCallback(
    async (nodeIds: string[]) => {
      if (!agent) return;
      const toDelete = nodeIds.map((id) => visibleNodeMap.get(id)).filter(Boolean) as NodeSnapshot[];
      if (toDelete.length === 0) return;
      try {
        await Promise.all(toDelete.map((node) => agent.nodes.removeNode(node.path)));
        await invalidateGraphQueries();
        setSelectedNodes([]);
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent, invalidateGraphQueries, setSelectedNodes, visibleNodeMap],
  );

  const deleteEdges = useCallback(
    async (edgeIds: string[]) => {
      if (!agent || edgeIds.length === 0) return;
      try {
        await Promise.all(edgeIds.map((id) => agent.links.remove(id)));
        // GraphStore handles cache removal via the SSE link_removed event.
        setSelectedEdges([]);
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent, setSelectedEdges],
  );

  const autoLayoutNodes = useCallback(async () => {
    if (!agent) return;
    const laidOut = autoLayout(visibleNodeList, edgeList);
    try {
      await Promise.all(
        laidOut.map((node) => agent.slots.writeSlot(node.data.snapshot.path, POSITION_SLOT, node.position)),
      );
      await queryClient.invalidateQueries({ queryKey: ["nodes"] });
      flowRef.current?.fitView({ duration: 250, padding: 0.18 });
      setErrorMessage(null);
    } catch (error) {
      setErrorMessage(formatError(error));
    }
  }, [agent, edgeList, visibleNodeList]);

  const handleSelectionChange = useCallback(
    ({ nodes: selectedNodes, edges: selectedEdges }: OnSelectionChangeParams) => {
      setSelectedNodes(
        selectedNodes
          .map((node) => ((node.data as FlowNodeData | undefined)?.snapshot.path))
          .filter((value): value is string => Boolean(value)),
      );
      setSelectedEdges(selectedEdges.map((edge) => edge.id));
    },
    [setSelectedEdges, setSelectedNodes],
  );

  const createNodeByKindId = useCallback(
    (kindId: string, position: { x: number; y: number }) => {
      const kind = kindsById.get(kindId);
      if (kind) {
        createNode(kind, position);
      }
    },
    [createNode, kindsById],
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Delete" && event.key !== "Backspace") {
        return;
      }
      const active = document.activeElement;
      if (
        active instanceof HTMLInputElement ||
        active instanceof HTMLTextAreaElement ||
        active instanceof HTMLSelectElement ||
        active?.getAttribute("contenteditable") === "true"
      ) {
        return;
      }
      event.preventDefault();
      void deleteEdges(selectedEdgeIds);
      const ids = visibleNodes
        .filter((node) => selectedNodePaths.includes(node.path))
        .map((node) => node.id);
      void deleteNodes(ids);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [deleteEdges, deleteNodes, selectedEdgeIds, selectedNodePaths, visibleNodes]);

  useEffect(
    () => () => {
      for (const timer of moveTimers.current.values()) {
        window.clearTimeout(timer);
      }
    },
    [],
  );

  return {
    flowRef,
    errorMessage,
    addChildPath,
    setAddChildPath,
    createNode,
    persistNodePosition,
    handleOpenNode,
    handleAddChildNode,
    handleConnect,
    deleteNodes,
    deleteEdges,
    autoLayoutNodes,
    handleSelectionChange,
    invalidateGraphQueries,
    createNodeByKindId,
  };
}
