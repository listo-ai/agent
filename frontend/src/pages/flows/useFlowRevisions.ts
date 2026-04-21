import { useQuery } from "@tanstack/react-query";
import { useAgent } from "@listo/ui-core";
import type { FlowRevisionDto } from "@listo/agent-client";

export interface UseFlowRevisionsOptions {
  /** Flow ULID. Pass `undefined` to disable the query. */
  flowId: string | undefined;
  limit?: number;
  offset?: number;
}

export interface UseFlowRevisionsResult {
  revisions: FlowRevisionDto[];
  total: number;
  isPending: boolean;
  isError: boolean;
  error: Error | null;
  refetch: () => void;
}

export function useFlowRevisions({
  flowId,
  limit = 50,
  offset = 0,
}: UseFlowRevisionsOptions): UseFlowRevisionsResult {
  const agent = useAgent();

  const query = useQuery({
    queryKey: ["flowRevisions", flowId, limit, offset],
    queryFn: () => agent.data!.flows.listRevisions(flowId!, { limit, offset }),
    enabled: !!agent.data && !!flowId,
    staleTime: 0,
  });

  return {
    revisions: query.data?.data ?? [],
    total: query.data?.total ?? 0,
    isPending: query.isPending,
    isError: query.isError,
    error: query.error,
    refetch: () => void query.refetch(),
  };
}
