import dagre from "dagre";
import type { Edge, Node, XYPosition } from "@xyflow/react";
import type { Kind, Link, NodeSnapshot, Slot } from "@acme/agent-client";

export const FLOW_KIND = "acme.core.flow";
export const POSITION_SLOT = "position";
export const DEFAULT_NODE_SIZE = { width: 220, height: 112 };

export interface LiveNodeState {
  lifecycle: NodeSnapshot["lifecycle"] | undefined;
  slots: Record<string, Slot> | undefined;
  touchedAt: number | undefined;
}

export type FlowNodeData = Record<string, unknown> & {
  snapshot: NodeSnapshot;
  kind: Kind | undefined;
};

export function isFlowNode(node: NodeSnapshot): boolean {
  return node.kind === FLOW_KIND;
}

export function isDirectChildOfFlow(node: NodeSnapshot, flowPath: string): boolean {
  return parentPath(node.path) === flowPath;
}

export function parentPath(path: string): string {
  if (path === "/") return "/";
  const trimmed = path.endsWith("/") && path.length > 1 ? path.slice(0, -1) : path;
  const idx = trimmed.lastIndexOf("/");
  return idx <= 0 ? "/" : trimmed.slice(0, idx);
}

export function basename(path: string): string {
  const trimmed = path.endsWith("/") && path.length > 1 ? path.slice(0, -1) : path;
  const idx = trimmed.lastIndexOf("/");
  return idx === -1 ? trimmed : trimmed.slice(idx + 1);
}

export function titleForKind(kind?: Kind, fallback?: string): string {
  return kind?.display_name ?? fallback ?? "Untitled";
}

export function facetGroup(kind: Kind): string {
  const facets = new Set(kind.facets as string[]);
  if (facets.has("isDriver") || facets.has("is_driver")) return "Driver";
  if (facets.has("isCompute") || facets.has("is_compute")) return "Compute";
  if (facets.has("isProtocol") || facets.has("is_protocol")) return "Protocol";
  if (facets.has("isSystem") || facets.has("is_system")) return "System";
  if (facets.has("isDevice") || facets.has("is_device")) return "Device";
  if (facets.has("isPoint") || facets.has("is_point")) return "Point";
  if (facets.has("isFlow") || facets.has("is_flow")) return "Flow";
  return "Other";
}

export function slotMap(slots: Slot[]): Record<string, Slot> {
  return Object.fromEntries(slots.map((slot) => [slot.name, slot]));
}

export function mergedSlots(snapshot: NodeSnapshot, live?: LiveNodeState): Slot[] {
  const base = slotMap(snapshot.slots);
  const next = { ...base, ...(live?.slots ?? {}) };
  return Object.values(next).sort((a, b) => a.name.localeCompare(b.name));
}

export function parsePosition(node: NodeSnapshot): XYPosition {
  const slot = node.slots.find((entry) => entry.name === POSITION_SLOT);
  const value = slot?.value;
  if (
    typeof value === "object" &&
    value !== null &&
    "x" in value &&
    "y" in value &&
    typeof value.x === "number" &&
    typeof value.y === "number"
  ) {
    return { x: value.x, y: value.y };
  }

  return { x: 80, y: 80 };
}

export function toCanvasNodes(
  nodes: NodeSnapshot[],
  kindsById: Map<string, Kind>,
): Node<FlowNodeData>[] {
  return nodes.map((snapshot) => ({
    id: snapshot.id,
    type: "flowNode",
    position: parsePosition(snapshot),
    data: {
      snapshot,
      kind: kindsById.get(snapshot.kind),
    },
  }));
}

export function toCanvasEdges(
  links: Link[],
  nodesById: Map<string, NodeSnapshot>,
): Edge[] {
  return links.flatMap((link) => {
    const source = nodesById.get(link.source.node_id);
    const target = nodesById.get(link.target.node_id);
    if (!source || !target) {
      return [];
    }

    return [
      {
        id: link.id,
        source: source.id,
        target: target.id,
        sourceHandle: link.source.slot,
        targetHandle: link.target.slot,
        animated: false,
      },
    ];
  });
}

export function autoLayout(nodes: Node<FlowNodeData>[], edges: Edge[]): Node<FlowNodeData>[] {
  const graph = new dagre.graphlib.Graph();
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({ rankdir: "LR", ranksep: 72, nodesep: 28, marginx: 24, marginy: 24 });

  for (const node of nodes) {
    graph.setNode(node.id, DEFAULT_NODE_SIZE);
  }

  for (const edge of edges) {
    graph.setEdge(edge.source, edge.target);
  }

  dagre.layout(graph);

  return nodes.map((node) => {
    const laidOut = graph.node(node.id);
    if (!laidOut) {
      return node;
    }

    return {
      ...node,
      position: {
        x: laidOut.x - DEFAULT_NODE_SIZE.width / 2,
        y: laidOut.y - DEFAULT_NODE_SIZE.height / 2,
      },
    };
  });
}

export function nextNodeName(kind: Kind, siblingPaths: string[]): string {
  const base = slugify(kind.display_name ?? basename(kind.id).replaceAll(".", "-")) || "node";
  const existing = new Set(siblingPaths.map((path) => basename(path)));
  if (!existing.has(base)) {
    return base;
  }

  let index = 2;
  while (existing.has(`${base}-${index}`)) {
    index += 1;
  }
  return `${base}-${index}`;
}

export function nextFlowName(flowPaths: string[]): string {
  const existing = new Set(flowPaths.map((path) => basename(path)));
  let index = 1;
  while (existing.has(`flow-${index}`)) {
    index += 1;
  }
  return `flow-${index}`;
}

export function formatLiveValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.length}]`;
  if (typeof value === "object") return "{…}";
  return "empty";
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}
