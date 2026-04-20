import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import "@xyflow/react/dist/style.css";
import { AGENT_BASE_URL } from "@/lib/agent";
import { CenteredMessage, formatError } from "./flow-page-shared";
import { FlowCanvas } from "./components/FlowCanvas";
import { FlowPropertyPanel } from "./components/FlowPropertyPanel";
import { NodeHistoryPanel } from "./components/NodeHistoryPanel";
import { FlowSidebar } from "./components/FlowSidebar";
import { FlowToolbar } from "./components/FlowToolbar";
import { AddChildNodeDialog } from "@/components/AddChildNodeDialog";
import { useFlowPageActions } from "./useFlowPageActions";
import { useFlowPageData } from "./useFlowPageData";

export function FlowsPage() {
  const navigate = useNavigate();
  // Router renders this at `/flows/edit/*` — `params["*"]` is the flow's
  // full graph path minus the leading slash. A hard refresh of the URL
  // deep-links straight to the canvas for that flow.
  const { "*": splat } = useParams();
  const pathFromUrl = splat ? `/${splat}` : null;

  // History panel: which node path's slot history is being viewed.
  const [historyPath, setHistoryPath] = useState<string | null>(null);

  const {
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
  } = useFlowPageData({ pathFromUrl, navigate });

  const {
    flowRef,
    errorMessage,
    addChildPath,
    setAddChildPath,
    createNode,
    persistNodePosition,
    saveSettings,
    handleOpenNode,
    handleAddChildNode,
    handleConnect,
    deleteNodes,
    deleteEdges,
    autoLayoutNodes,
    handleSelectionChange,
    invalidateGraphQueries,
    createNodeByKindId,
  } = useFlowPageActions({
    agent: agent.data,
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
  });

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
                onCreateNodeFromKindId={createNodeByKindId}
                onPersistNodePosition={persistNodePosition}
                onConnect={(connection) => void handleConnect(connection)}
                onDeleteNodes={(nodeIds) => void deleteNodes(nodeIds)}
                onDeleteEdges={(edgeIds) => void deleteEdges(edgeIds)}
                onSelectionChange={handleSelectionChange}
                onReady={(instance) => {
                  flowRef.current = instance;
                }}
                onOpenNode={handleOpenNode}
                onAddChildNode={handleAddChildNode}
                onOpenHistory={(nodePath) => setHistoryPath(nodePath)}
              />
            ) : (
              <CenteredMessage
                title="No flow selected"
                detail="Create a flow container, then drag kinds onto the canvas."
              />
            )}
          </div>

          {historyPath ? (
            <NodeHistoryPanel
              node={visibleNodes.find((n) => n.path === historyPath)}
              onClose={() => setHistoryPath(null)}
            />
          ) : (
            <FlowPropertyPanel
              node={selectedNode}
              kind={selectedKind}
              live={selectedLive}
              onSaveSettings={saveSettings}
            />
          )}
        </div>
      </section>

      {addChildPath && agent.data && (
        <AddChildNodeDialog
          parentPath={addChildPath}
          agent={agent.data}
          onClose={() => setAddChildPath(null)}
          onCreated={async () => {
            setAddChildPath(null);
            await invalidateGraphQueries();
          }}
        />
      )}
    </div>
  );
}
