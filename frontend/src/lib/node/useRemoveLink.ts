/**
 * Mutation hook for removing a link by id.
 *
 * Note: the GraphStore drives cache removal via SSE `link_removed` events, so
 * we only invalidate the React Query `links` cache here (not nodes).
 *
 * @example
 *   const removeLink = useRemoveLink();
 *   removeLink.mutate("link-id-123");
 */
import { useMutation } from "@tanstack/react-query";
import { useAgent } from "@listo/ui-core";
import { queryClient } from "@listo/ui-core";

export function useRemoveLink() {
  const agentQuery = useAgent();

  return useMutation({
    mutationFn: async (id: string): Promise<void> => {
      await agentQuery.data!.links.remove(id);
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["links"] }),
  });
}
