/**
 * Mutation hook for creating a link between two node slots.
 *
 * Automatically invalidates the graph cache (links) on success.
 * Returns the new link id as mutation data.
 *
 * @example
 *   const createLink = useCreateLink();
 *   createLink.mutate({
 *     source: { path: "/flows/f/a", slot: "out" },
 *     target: { path: "/flows/f/b", slot: "in" },
 *   });
 */
import { useMutation } from "@tanstack/react-query";
import type { LinkEndpointRef } from "@listo/agent-client";
import { useAgent } from "@/hooks/useAgent";
import { queryClient } from "@/providers/query";

export interface CreateLinkInput {
  source: LinkEndpointRef;
  target: LinkEndpointRef;
}

export function useCreateLink() {
  const agentQuery = useAgent();

  return useMutation({
    mutationFn: async ({ source, target }: CreateLinkInput): Promise<string> => {
      return agentQuery.data!.links.create(source, target);
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["links"] }),
  });
}
