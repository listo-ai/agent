import { createContext, memo, useContext, useEffect, useMemo, useState } from "react";
import {
  addEdge,
  Background,
  ConnectionLineType,
  Controls,
  Handle,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
  type NodeProps,
  type OnNodeDrag,
  type OnSelectionChangeParams,
  type ReactFlowInstance,
} from "@xyflow/react";
import { cn } from "@/lib/utils";
import {
  formatLiveValue,
  mergedSlots,
  titleForKind,
  type FlowNodeData,
  type LiveNodeState,
} from "../flow-model";

const LiveDataContext = createContext<Record<string, LiveNodeState>>({});

interface FlowCanvasProps {
  nodes: Node<FlowNodeData>[];
  edges: Edge[];
  liveByPath: Record<string, LiveNodeState>;
  selectedNodePaths: string[];
  selectedEdgeIds: string[];
  showGrid: boolean;
  showMiniMap: boolean;
  onCreateNodeFromKindId: (kindId: string, position: { x: number; y: number }) => void;
  onPersistNodePosition: (node: Node<FlowNodeData>) => void;
  onConnect: (connection: Connection) => void;
  onDeleteNodes: (nodeIds: string[]) => void;
  onDeleteEdges: (edgeIds: string[]) => void;
  onSelectionChange: (params: OnSelectionChangeParams) => void;
  onReady: (instance: ReactFlowInstance<Node<FlowNodeData>, Edge>) => void;
}

export function FlowCanvas(props: FlowCanvasProps) {
  return (
    <ReactFlowProvider>
      <LiveDataContext value={props.liveByPath}>
        <FlowCanvasInner {...props} />
      </LiveDataContext>
    </ReactFlowProvider>
  );
}

function FlowCanvasInner({
  nodes,
  edges,
  liveByPath: _liveByPath,
  selectedNodePaths,
  selectedEdgeIds,
  showGrid,
  showMiniMap,
  onCreateNodeFromKindId,
  onPersistNodePosition,
  onConnect,
  onDeleteNodes,
  onDeleteEdges,
  onSelectionChange,
  onReady,
}: FlowCanvasProps) {
  const [canvasNodes, setCanvasNodes] = useState(nodes);
  const [canvasEdges, setCanvasEdges] = useState(edges);

  useEffect(() => {
    // Preserve locally-dragged positions: only take the server position for
    // nodes that don't yet exist in local state (newly added). For existing
    // nodes keep whatever position React Flow currently has — otherwise a
    // query-refetch (triggered after every drag-stop) resets ALL nodes back
    // to their last-persisted coordinates mid-interaction.
    setCanvasNodes((current) => {
      const localById = new Map(current.map((n) => [n.id, n]));
      return nodes.map((n) => ({
        ...n,
        position: localById.get(n.id)?.position ?? n.position,
        selected: localById.get(n.id)?.selected ?? n.selected ?? false,
      }));
    });
  }, [nodes]);
  useEffect(() => setCanvasEdges(edges), [edges]);

  const selectedEdgesSet = useMemo(() => new Set(selectedEdgeIds), [selectedEdgeIds]);

  const visualEdges = useMemo<Edge[]>(
    () =>
      canvasEdges.map((edge) => ({
        ...edge,
        selected: selectedEdgesSet.has(edge.id),
      })),
    [canvasEdges, selectedEdgesSet],
  );

  const nodeTypes = useMemo(() => ({ flowNode: FlowNodeCard }), []);

  const onNodesChange = (changes: NodeChange<Node<FlowNodeData>>[]) => {
    setCanvasNodes((current) => applyNodeChanges<Node<FlowNodeData>>(changes, current));
  };

  const onEdgesChange = (changes: EdgeChange<Edge>[]) => {
    setCanvasEdges((current) => applyEdgeChanges(changes, current));
  };

  const onNodeDragStop: OnNodeDrag<Node<FlowNodeData>> = (_event, node) => {
    onPersistNodePosition(node);
  };

  return (
    <div className="h-full w-full bg-[radial-gradient(circle_at_top,#eff6ff,transparent_55%)]">
      <ReactFlow
        nodes={canvasNodes}
        edges={visualEdges}
        nodeTypes={nodeTypes}
        fitView
        minZoom={0.25}
        maxZoom={1.8}
        onInit={onReady}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeDragStop={onNodeDragStop}
        onConnect={(connection) => {
          setCanvasEdges((current) =>
            addEdge({ ...connection, type: "smoothstep", animated: false }, current),
          );
          onConnect(connection);
        }}
        onSelectionChange={onSelectionChange}
        onNodesDelete={(deleted) => onDeleteNodes(deleted.map((node) => node.id))}
        onEdgesDelete={(deleted) => onDeleteEdges(deleted.map((edge) => edge.id))}
        deleteKeyCode={null}
        selectionOnDrag
        multiSelectionKeyCode="Shift"
        connectionLineType={ConnectionLineType.SmoothStep}
        onDragOver={(event) => {
          event.preventDefault();
          event.dataTransfer.dropEffect = "copy";
        }}
        onDrop={(event) => {
          event.preventDefault();
          const raw = event.dataTransfer.getData("application/x-flow-kind");
          if (!raw) return;
          const parsed = JSON.parse(raw) as { kindId?: string };
          if (!parsed.kindId) return;
          const rect = (event.currentTarget as HTMLDivElement).getBoundingClientRect();
          onCreateNodeFromKindId(parsed.kindId, {
            x: event.clientX - rect.left,
            y: event.clientY - rect.top,
          });
        }}
        defaultEdgeOptions={{ type: "smoothstep" }}
      >
        {showGrid ? <Background gap={20} size={1} color="hsl(var(--border))" /> : null}
        <Controls showInteractive={false} className="rounded-lg border border-border bg-background/90 shadow-sm" />
        {showMiniMap ? (
          <MiniMap
            pannable
            zoomable
            className="rounded-lg border border-border bg-background/90 shadow-sm"
            nodeStrokeWidth={2}
            maskColor="rgba(15, 23, 42, 0.08)"
          />
        ) : null}
      </ReactFlow>
    </div>
  );
}

// Layout constants for handle positioning
const CARD_PADDING_TOP = 12; // pt-3
const HEADER_HEIGHT = 52;    // title line + kind line + gaps
const SLOT_ROW_H = 22;       // height per slot row
const SLOTS_PADDING_TOP = 6; // gap before slot rows

const FlowNodeCard = memo(function FlowNodeCard({
  data,
  selected,
}: NodeProps<Node<FlowNodeData>>) {
  const liveByPath = useContext(LiveDataContext);
  const live = liveByPath[data.snapshot.path];
  const slots = mergedSlots(data.snapshot, live);
  const inputs = data.kind?.slots.filter((slot) => slot.role === "input") ?? [];
  const outputs = data.kind?.slots.filter((slot) => slot.role === "output") ?? [];
  const statusSlots = slots.filter((slot) =>
    (data.kind?.slots.find((entry) => entry.name === slot.name)?.role ?? "status") === "status",
  );

  const slotRows = Math.max(inputs.length, outputs.length);
  const slotsBlockTop = CARD_PADDING_TOP + HEADER_HEIGHT + SLOTS_PADDING_TOP;

  return (
    <div
      className={cn(
        "relative rounded-2xl border bg-background/95 shadow-sm transition-shadow",
        selected ? "border-primary shadow-md" : "border-border",
      )}
      style={{ width: 240 }}
    >
      {/* Handles – positioned absolutely relative to card top */}
      {inputs.map((slot, index) => (
        <Handle
          key={`in-${slot.name}`}
          id={slot.name}
          type="target"
          position={Position.Left}
          style={{ top: slotsBlockTop + index * SLOT_ROW_H + SLOT_ROW_H / 2 }}
          className="!h-3 !w-3 !border-2 !border-background !bg-slate-500"
        />
      ))}
      {outputs.map((slot, index) => (
        <Handle
          key={`out-${slot.name}`}
          id={slot.name}
          type="source"
          position={Position.Right}
          style={{ top: slotsBlockTop + index * SLOT_ROW_H + SLOT_ROW_H / 2 }}
          className="!h-3 !w-3 !border-2 !border-background !bg-blue-500"
        />
      ))}

      {/* Header */}
      <div className="flex items-start justify-between gap-2 px-4 pt-3 pb-1">
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-semibold leading-snug">
            {titleForKind(data.kind, data.snapshot.path.split("/").pop())}
          </div>
          <div className="truncate text-[11px] text-muted-foreground">{data.snapshot.kind}</div>
        </div>
        <span className="mt-0.5 shrink-0 rounded-full bg-secondary px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-secondary-foreground">
          {live?.lifecycle ?? data.snapshot.lifecycle}
        </span>
      </div>

      {/* Slot labels row */}
      {slotRows > 0 && (
        <div
          className="relative mx-0 border-t border-border/40"
          style={{ height: slotRows * SLOT_ROW_H + SLOTS_PADDING_TOP * 2 }}
        >
          {inputs.map((slot, index) => (
            <span
              key={`label-in-${slot.name}`}
              className="pointer-events-none absolute left-5 text-[10px] font-medium uppercase tracking-wide text-slate-500"
              style={{ top: SLOTS_PADDING_TOP + index * SLOT_ROW_H + SLOT_ROW_H / 2, transform: "translateY(-50%)" }}
            >
              {slot.name}
            </span>
          ))}
          {outputs.map((slot, index) => (
            <span
              key={`label-out-${slot.name}`}
              className="pointer-events-none absolute right-5 text-[10px] font-medium uppercase tracking-wide text-blue-600"
              style={{ top: SLOTS_PADDING_TOP + index * SLOT_ROW_H + SLOT_ROW_H / 2, transform: "translateY(-50%)" }}
            >
              {slot.name}
            </span>
          ))}
        </div>
      )}

      {/* Status chips */}
      {statusSlots.length > 0 && (
        <div className="flex flex-wrap gap-2 border-t border-border/40 px-4 py-2">
          {statusSlots.slice(0, 4).map((slot) => (
            <span
              key={slot.name}
              className="rounded-full bg-slate-100 px-2 py-0.5 text-[11px] font-medium text-slate-700"
            >
              {slot.name}: {formatLiveValue(slot.value)}
            </span>
          ))}
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center justify-between border-t border-border/40 px-4 py-2 text-[11px] text-muted-foreground">
        <span className="truncate">{data.snapshot.path}</span>
        <span className="ml-2 shrink-0">{inputs.length} in · {outputs.length} out</span>
      </div>
    </div>
  );
});
