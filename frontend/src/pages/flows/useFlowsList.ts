import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation } from "@tanstack/react-query";
import { create, useStore } from "zustand";
import type { NodeSnapshot, Link } from "@sys/agent-client";

import { useAgent } from "@/hooks/useAgent";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { AGENT_BASE_URL } from "@/lib/agent";
import {
  basename,
  isDirectChildOfFlow,
  isFlowNode,
  nextFlowName,
} from "./flow-model";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FlowRow {
  id: string;
  path: string;
  name: string;
  lifecycle: string;
  nodeCount: number;
  linkCount: number;
}

export type ListStatus = "loading" | "error" | "ready";

export interface UseFlowsListReturn {
  status: ListStatus;
  errorDetail: string;
  rows: FlowRow[];
  filter: string;
  setFilter: (v: string) => void;
  errorMessage: string | null;
  createFlow: () => void;
  createPending: boolean;
  deleteFlow: (path: string) => void;
  deletePending: boolean;
  openFlow: (path: string) => void;
}

// ---------------------------------------------------------------------------
// Stable empty store (used before GraphStore connects)
// ---------------------------------------------------------------------------

const emptyStore = create(() => ({
  nodes: new Map() as Map<string, NodeSnapshot>,
  links: new Map() as Map<string, Link>,
}));

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

/**
 * Encapsulates **all** business logic for the flows list page.
 * The page component only calls this hook and passes the return value
 * to a pure presentational layer.
 */
export function useFlowsList(): UseFlowsListReturn {
  const navigate = useNavigate();
  const agent = useAgent();
  const graphStore = useGraphStoreOptional();

  useEffect(() => {
    if (!graphStore) return;
    graphStore.getState().expand("/");
  }, [graphStore]);

  const activeStore = graphStore ?? emptyStore;
  const nodeMap = useStore(activeStore, (s) => s.nodes);
  const linkMap = useStore(activeStore, (s) => s.links);

  const nodes = useMemo(() => [...nodeMap.values()], [nodeMap]);
  const links = useMemo(() => [...linkMap.values()], [linkMap]);

  const [filter, setFilter] = useState("");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Derive rows
  const rows = useMemo(() => {
    const flows = nodes.filter(isFlowNode);
    return flows
      .map((flow) => {
        const children = nodes.filter((n) =>
          isDirectChildOfFlow(n, flow.path),
        );
        const childIds = new Set(children.map((n) => n.id));
        const linkCount = links.filter(
          (l) =>
            childIds.has(l.source.node_id) && childIds.has(l.target.node_id),
        ).length;
        return {
          id: flow.id,
          path: flow.path,
          name: basename(flow.path),
          lifecycle: flow.lifecycle,
          nodeCount: children.length,
          linkCount,
        };
      })
      .filter((row) => {
        if (!filter.trim()) return true;
        const t = filter.trim().toLowerCase();
        return (
          row.name.toLowerCase().includes(t) ||
          row.path.toLowerCase().includes(t)
        );
      })
      .sort((a, b) => a.path.localeCompare(b.path));
  }, [nodes, links, filter]);

  // Mutations
  const createFlowMutation = useMutation({
    mutationFn: async () => {
      const name = nextFlowName(nodes.filter(isFlowNode).map((f) => f.path));
      const created = await agent.data!.nodes.createNode({
        parent: "/",
        kind: "sys.core.flow",
        name,
      });
      return created.path;
    },
    onSuccess: (path) => {
      setErrorMessage(null);
      navigate(`/flows/edit${path}`);
    },
    onError: (error) => setErrorMessage(formatError(error)),
  });

  const deleteFlowMutation = useMutation({
    mutationFn: async (path: string) => {
      await agent.data!.nodes.removeNode(path);
    },
    onSuccess: () => setErrorMessage(null),
    onError: (error) => setErrorMessage(formatError(error)),
  });

  // Determine overall status
  let status: ListStatus = "ready";
  if (!graphStore || agent.isPending) status = "loading";
  else if (agent.isError) status = "error";

  return {
    status,
    errorDetail: agent.isError
      ? `${AGENT_BASE_URL} — ${formatError(agent.error)}`
      : AGENT_BASE_URL,
    rows,
    filter,
    setFilter,
    errorMessage,
    createFlow: () => createFlowMutation.mutate(),
    createPending: createFlowMutation.isPending,
    deleteFlow: (path) => {
      if (
        confirm(
          `Delete flow ${path}? This removes the container and all child nodes.`,
        )
      ) {
        deleteFlowMutation.mutate(path);
      }
    },
    deletePending: deleteFlowMutation.isPending,
    openFlow: (path) => navigate(`/flows/edit${path}`),
  };
}

// ---------------------------------------------------------------------------
// Util
// ---------------------------------------------------------------------------

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}
