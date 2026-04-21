import { useMemo } from "react";
import type { NodeSnapshot } from "@listo/agent-client";

/**
 * Resolves a flow's node-path (e.g. `/flow-1`) to its ULID from the
 * GraphStore's node cache — no extra HTTP round-trip.
 *
 * The ULID is required for the `/api/v1/flows/{id}` REST surface
 * (undo/redo, revision list, etc.).
 */
export function useFlowId(
  nodes: NodeSnapshot[],
  flowPath: string | null,
): string | undefined {
  return useMemo(
    () => (flowPath ? nodes.find((n) => n.path === flowPath)?.id : undefined),
    [nodes, flowPath],
  );
}
