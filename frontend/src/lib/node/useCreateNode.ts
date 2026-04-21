/**
 * Mutation hook for creating a node.
 *
 * Automatically invalidates the graph cache on success.
 * The returned mutation's `data` is the new node path.
 *
 * @example
 *   const createNode = useCreateNode();
 *   createNode.mutate(
 *     { parent: "/flows/my-flow", kind: "sys.core.counter", name: "counter-1" },
 *     { onSuccess: (path) => navigate(`/flows/edit${path}`) },
 *   );
 */
import { useMutation } from "@tanstack/react-query";
import { useAgent } from "@listo/ui-core";
import { useInvalidateGraph } from "./useInvalidateGraph";

export interface CreateNodeInput {
  parent: string;
  kind: string;
  name: string;
}

export function useCreateNode() {
  const agentQuery = useAgent();
  const invalidate = useInvalidateGraph();

  return useMutation({
    mutationFn: async (input: CreateNodeInput): Promise<string> => {
      const created = await agentQuery.data!.nodes.createNode(input);
      return created.path;
    },
    onSuccess: () => invalidate(),
  });
}
