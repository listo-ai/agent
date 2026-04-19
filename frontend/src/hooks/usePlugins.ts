import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { PluginSummary } from "@sys/agent-client";

import { useAgent } from "./useAgent";

const PLUGINS_KEY = ["plugins"] as const;

/** List every plugin discovered by `PluginRegistry::scan` on the agent. */
export function usePlugins() {
  const agent = useAgent();
  return useQuery<PluginSummary[]>({
    queryKey: PLUGINS_KEY,
    queryFn: () => agent.data!.plugins.list(),
    enabled: agent.data !== undefined,
  });
}

/**
 * Enable / disable / reload mutations. All invalidate `PLUGINS_KEY`
 * on success so the list reflects the new state.
 */
export function usePluginMutations() {
  const agent = useAgent();
  const qc = useQueryClient();

  const invalidate = () => qc.invalidateQueries({ queryKey: PLUGINS_KEY });

  const enable = useMutation({
    mutationFn: (id: string) => agent.data!.plugins.enable(id),
    onSuccess: invalidate,
  });
  const disable = useMutation({
    mutationFn: (id: string) => agent.data!.plugins.disable(id),
    onSuccess: invalidate,
  });
  const reload = useMutation({
    mutationFn: () => agent.data!.plugins.reload(),
    onSuccess: invalidate,
  });

  return { enable, disable, reload };
}
