import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import type { Kind, NodeConfig, NodeSnapshot, Slot } from "@acme/agent-client";
import type { Connection, Edge, Node, OnSelectionChangeParams, ReactFlowInstance } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMutation } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { useAgent, useKinds, useLinks, useNodes } from "@/hooks/useAgent";
import { AGENT_BASE_URL } from "@/lib/agent";
import { queryClient } from "@/providers/query";
import { useFlowStore } from "@/store/flow";
import { FlowCanvas } from "./components/FlowCanvas";
import { FlowPropertyPanel } from "./components/FlowPropertyPanel";
import { FlowSidebar } from "./components/FlowSidebar";
import { FlowToolbar } from "./components/FlowToolbar";
import {
  autoLayout,
  isDirectChildOfFlow,
  isFlowNode,
  nextFlowName,
  nextNodeName,
  POSITION_SLOT,
  toCanvasEdges,
  toCanvasNodes,
  type FlowNodeData,
} from "./flow-model";
import { useFlowLiveData } from "./useFlowLiveData";

export function FlowsPage() {
  const navigate = useNavigate();
  // Router renders this at `/flows/edit/*` — `params["*"]` is the flow's
  // full graph path minus the leading slash. A hard refresh of the URL
  // deep-links straight to the canvas for that flow.
  const { "*": splat } = useParams();
  const pathFromUrl = splat ? `/${splat}` : null;

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

  const flowRef = useRef<ReactFlowInstance<Node<FlowNodeData>, Edge> | null>(null);
  const moveTimers = useRef(new Map<string, number>());
  const configTimers = useRef(new Map<string, number>());
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const nodes = nodesQuery.data ?? [];
  const links = linksQuery.data ?? [];
  const kinds = kindsQuery.data ?? [];
  const kindsById = useMemo(() => new Map(kinds.map((kind) => [kind.id, kind])), [kinds]);
  const flowNodes = useMemo(() => nodes.filter(isFlowNode), [nodes]);

  // URL is the source of truth for which flow is open. Sync the store
  // from it, and if the URL's flow disappears (deleted elsewhere) bounce
  // back to the list.
  useEffect(() => {
    if (!pathFromUrl) return;
    const exists = flowNodes.some((flow) => flow.path === pathFromUrl);
    if (!exists) {
      // Wait for queries to resolve before giving up — avoids bouncing
      // on the first render while nodes are still loading.
      if (!nodesQuery.isPending) {
        navigate("/flows", { replace: true });
      }
      return;
    }
    if (openFlowPath !== pathFromUrl) {
      setOpenFlow(pathFromUrl);
    }
  }, [pathFromUrl, flowNodes, nodesQuery.isPending, openFlowPath, setOpenFlow, navigate]);

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

  const liveByPath = useFlowLiveData(agent.data, nodes);
  const visibleNodeList = useMemo(
    () => toCanvasNodes(visibleNodes, kindsById),
    [visibleNodes, kindsById],
  );
  const visibleNodeMap = useMemo(() => new Map(visibleNodes.map((node) => [node.id, node])), [visibleNodes]);
  const edgeList = useMemo(() => toCanvasEdges(visibleLinks, visibleNodeMap), [visibleLinks, visibleNodeMap]);

  const selectedNode = useMemo(
    () => nodes.find((node) => selectedNodePaths.includes(node.path)),
    [nodes, selectedNodePaths],
  );
  const selectedKind = selectedNode ? kindsById.get(selectedNode.kind) : undefined;
  const selectedLive: Record<string, Slot> = selectedNode ? (liveByPath[selectedNode.path]?.slots ?? {}) : {};

  // createFlow stays available for parity with the list page, but today
  // only the list page calls it — kept here for future "duplicate this
  // flow" affordances on the toolbar.
  void flowNodes;

  const createNodeMutation = useMutation({
    mutationFn: async ({ kind, position }: { kind: Kind; position: { x: number; y: number } }) => {
      if (!openFlowPath) return;
      const name = nextNodeName(kind, visibleNodes.map((node) => node.path));
      const created = await agent.data!.nodes.createNode({
        parent: openFlowPath,
        kind: kind.id,
        name,
      });
      await agent.data!.slots.writeSlot(created.path, "position", position);
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["nodes"] }),
        queryClient.invalidateQueries({ queryKey: ["links"] }),
      ]);
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
      const path = node.data.snapshot.path;
      const previous = moveTimers.current.get(path);
      if (previous) {
        window.clearTimeout(previous);
      }
      const timer = window.setTimeout(async () => {
        try {
          await agent.data!.slots.writeSlot(path, "position", node.position);
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
    [agent.data],
  );

  const saveConfig = useCallback(
    (path: string, config: NodeConfig) => {
      const previous = configTimers.current.get(path);
      if (previous) {
        window.clearTimeout(previous);
      }
      const timer = window.setTimeout(async () => {
        try {
          await agent.data!.config.setConfig(path, config);
          setErrorMessage(null);
        } catch (error) {
          setErrorMessage(formatError(error));
        } finally {
          configTimers.current.delete(path);
        }
      }, 500);
      configTimers.current.set(path, timer);
    },
    [agent.data],
  );

  const handleConnect = useCallback(
    async (connection: Connection) => {
      if (!connection.source || !connection.target || !connection.sourceHandle || !connection.targetHandle) {
        return;
      }
      const source = visibleNodeMap.get(connection.source);
      const target = visibleNodeMap.get(connection.target);
      if (!source || !target) {
        return;
      }

      try {
        await agent.data!.links.create(
          { path: source.path, slot: connection.sourceHandle },
          { path: target.path, slot: connection.targetHandle },
        );
        await queryClient.invalidateQueries({ queryKey: ["links"] });
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent.data, visibleNodeMap],
  );

  const deleteNodes = useCallback(
    async (nodeIds: string[]) => {
      const toDelete = nodeIds.map((id) => visibleNodeMap.get(id)).filter(Boolean) as NodeSnapshot[];
      if (toDelete.length === 0) return;
      try {
        await Promise.all(toDelete.map((node) => agent.data!.nodes.removeNode(node.path)));
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: ["nodes"] }),
          queryClient.invalidateQueries({ queryKey: ["links"] }),
        ]);
        setSelectedNodes([]);
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent.data, setSelectedNodes, visibleNodeMap],
  );

  const deleteEdges = useCallback(
    async (edgeIds: string[]) => {
      if (edgeIds.length === 0) return;
      try {
        await Promise.all(edgeIds.map((id) => agent.data!.links.remove(id)));
        await queryClient.invalidateQueries({ queryKey: ["links"] });
        setSelectedEdges([]);
        setErrorMessage(null);
      } catch (error) {
        setErrorMessage(formatError(error));
      }
    },
    [agent.data, setSelectedEdges],
  );

  const autoLayoutNodes = useCallback(async () => {
    const laidOut = autoLayout(visibleNodeList, edgeList);
    try {
      await Promise.all(
        laidOut.map((node) =>
          agent.data!.slots.writeSlot(node.data.snapshot.path, POSITION_SLOT, node.position),
        ),
      );
      await queryClient.invalidateQueries({ queryKey: ["nodes"] });
      flowRef.current?.fitView({ duration: 250, padding: 0.18 });
      setErrorMessage(null);
    } catch (error) {
      setErrorMessage(formatError(error));
    }
  }, [agent.data, edgeList, visibleNodeList]);

  const handleSelectionChange = useCallback(
    ({ nodes: selectedNodes, edges: selectedEdges }: OnSelectionChangeParams) => {
      setSelectedNodes(
        selectedNodes
          .map((node) => ((node.data as unknown as FlowNodeData | undefined)?.snapshot.path))
          .filter((value): value is string => Boolean(value)),
      );
      setSelectedEdges(selectedEdges.map((edge) => edge.id));
    },
    [setSelectedEdges, setSelectedNodes],
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
      for (const timer of configTimers.current.values()) {
        window.clearTimeout(timer);
      }
    },
    [],
  );

  if (nodesQuery.isPending || linksQuery.isPending || kindsQuery.isPending) {
    return <CenteredMessage title="Loading flows…" detail={AGENT_BASE_URL} />;
  }

  if (nodesQuery.isError || linksQuery.isError || kindsQuery.isError || agent.isError) {
    return (
      <CenteredMessage
        title="Could not reach the agent"
        detail={`${AGENT_BASE_URL} — ${formatError(nodesQuery.error ?? linksQuery.error ?? kindsQuery.error ?? agent.error)}`}
      />
    );
  }

  return (
    <div className="flex h-full min-h-0 bg-background">
      <FlowSidebar kinds={kinds} onCreateNode={createNode} />

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-2 border-b border-border bg-card/40 px-4 py-2">
          <button
            type="button"
            onClick={() => navigate("/flows")}
            className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
          >
            <ArrowLeft size={12} />
            All flows
          </button>
          <span className="font-mono text-xs text-muted-foreground">
            {openFlowPath ?? "…"}
          </span>
        </div>

        <FlowToolbar
          selectedFlowPath={openFlowPath}
          nodeCount={visibleNodes.length}
          onAutoLayout={() => void autoLayoutNodes()}
          onFitView={() => flowRef.current?.fitView({ duration: 250, padding: 0.18 })}
          onZoomIn={() => flowRef.current?.zoomIn({ duration: 180 })}
          onZoomOut={() => flowRef.current?.zoomOut({ duration: 180 })}
          onToggleMiniMap={toggleMiniMap}
          onToggleGrid={toggleGrid}
          showMiniMap={showMiniMap}
          showGrid={showGrid}
        />

        {errorMessage ? (
          <div className="border-b border-destructive/20 bg-destructive/8 px-4 py-2 text-sm text-destructive">
            {errorMessage}
          </div>
        ) : null}

        <div className="min-h-0 flex flex-1">
          <div className="min-w-0 flex-1">
            {openFlowPath ? (
              <FlowCanvas
                nodes={visibleNodeList}
                edges={edgeList}
                liveByPath={liveByPath}
                selectedNodePaths={selectedNodePaths}
                selectedEdgeIds={selectedEdgeIds}
                showGrid={showGrid}
                showMiniMap={showMiniMap}
                onCreateNodeFromKindId={(kindId, position) => {
                  const kind = kindsById.get(kindId);
                  if (kind) {
                    createNode(kind, position);
                  }
                }}
                onPersistNodePosition={persistNodePosition}
                onConnect={(connection) => void handleConnect(connection)}
                onDeleteNodes={(nodeIds) => void deleteNodes(nodeIds)}
                onDeleteEdges={(edgeIds) => void deleteEdges(edgeIds)}
                onSelectionChange={handleSelectionChange}
                onReady={(instance) => {
                  flowRef.current = instance;
                }}
              />
            ) : (
              <CenteredMessage
                title="No flow selected"
                detail="Create a flow container, then drag kinds onto the canvas."
              />
            )}
          </div>

          <FlowPropertyPanel
            node={selectedNode}
            kind={selectedKind}
            live={selectedLive}
            onSaveConfig={saveConfig}
          />
        </div>
      </section>
    </div>
  );
}

function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
      <p className="text-sm">{title}</p>
      <p className="max-w-md text-xs font-mono">{detail}</p>
    </div>
  );
}

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}
