/**
 * Mutation hook for deleting a node by path.
 *
 * Automatically invalidates the graph cache on success.
 *
 * @example
 *   const removeNode = useRemoveNode();
 *   removeNode.mutate("/flows/my-flow/counter-1");
 */
import { useMutation } from "@tanstack/react-query";
import { useAgent } from "@listo/ui-core";
import { useInvalidateGraph } from "./useInvalidateGraph";

export function useRemoveNode() {
  const agentQuery = useAgent();
  const invalidate = useInvalidateGraph();

  return useMutation({
    mutationFn: async (path: string): Promise<void> => {
      await agentQuery.data!.nodes.removeNode(path);
    },
    onSuccess: () => invalidate(),
  });
}
