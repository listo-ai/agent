import { useCallback, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useAgent } from "@/hooks/useAgent";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { formatError } from "@/lib/utils";

export interface UseFlowUndoRedoOptions {
  /** ULID of the open flow. Use `useFlowId` to obtain it. */
  flowId: string | undefined;
  /** Current head revision id — OCC guard forwarded to the server. */
  headRevisionId: string | undefined;
  /** Called after every successful undo/redo/revert with the new head id. */
  onHeadChange?: (newHeadRevisionId: string) => void;
}

export interface UseFlowUndoRedoResult {
  isPending: boolean;
  errorMessage: string | null;
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  revert: (targetRevId: string) => void;
}

function is422(err: unknown): boolean {
  return !!(err && typeof err === "object" && "status" in err && (err as { status: number }).status === 422);
}

export function useFlowUndoRedo({
  flowId,
  headRevisionId,
  onHeadChange,
}: UseFlowUndoRedoOptions): UseFlowUndoRedoResult {
  const agent = useAgent();
  const graphStore = useGraphStoreOptional();
  const queryClient = useQueryClient();

  const [canUndo, setCanUndo] = useState(true);
  const [canRedo, setCanRedo] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const afterMutation = useCallback(
    (newHeadId: string) => {
      onHeadChange?.(newHeadId);
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ["nodes"] }),
        queryClient.invalidateQueries({ queryKey: ["links"] }),
        queryClient.invalidateQueries({ queryKey: ["flowRevisions", flowId] }),
      ]);
      graphStore?.getState().reconcile();
      setErrorMessage(null);
    },
    [flowId, graphStore, queryClient, onHeadChange],
  );

  const undoMutation = useMutation({
    mutationFn: async () => {
      if (!agent.data || !flowId) return;
      return agent.data.flows.undo({
        id: flowId,
        ...(headRevisionId !== undefined && { expectedHead: headRevisionId }),
      });
    },
    onSuccess: (result) => {
      if (!result) return;
      setCanUndo(true);
      setCanRedo(true);
      afterMutation(result.head_revision_id);
    },
    onError: (err) => {
      if (is422(err)) setCanUndo(false);
      setErrorMessage(formatError(err));
    },
  });

  const redoMutation = useMutation({
    mutationFn: async () => {
      if (!agent.data || !flowId) return;
      return agent.data.flows.redo({
        id: flowId,
        ...(headRevisionId !== undefined && { expectedHead: headRevisionId }),
      });
    },
    onSuccess: (result) => {
      if (!result) return;
      setCanUndo(true);
      afterMutation(result.head_revision_id);
    },
    onError: (err) => {
      if (is422(err)) setCanRedo(false);
      setErrorMessage(formatError(err));
    },
  });

  const revertMutation = useMutation({
    mutationFn: async (targetRevId: string) => {
      if (!agent.data || !flowId) return;
      return agent.data.flows.revert({
        id: flowId,
        targetRevId,
        ...(headRevisionId !== undefined && { expectedHead: headRevisionId }),
      });
    },
    onSuccess: (result) => {
      if (!result) return;
      setCanUndo(true);
      setCanRedo(false);
      afterMutation(result.head_revision_id);
    },
    onError: (err) => setErrorMessage(formatError(err)),
  });

  const isPending = undoMutation.isPending || redoMutation.isPending || revertMutation.isPending;

  return {
    isPending,
    errorMessage,
    canUndo: canUndo && !!flowId && !isPending,
    canRedo: canRedo && !!flowId && !isPending,
    undo: () => {
      if (!flowId || isPending) return;
      setErrorMessage(null);
      undoMutation.mutate();
    },
    redo: () => {
      if (!flowId || isPending) return;
      setErrorMessage(null);
      redoMutation.mutate();
    },
    revert: (targetRevId) => {
      if (!flowId || isPending) return;
      setErrorMessage(null);
      revertMutation.mutate(targetRevId);
    },
  };
}
