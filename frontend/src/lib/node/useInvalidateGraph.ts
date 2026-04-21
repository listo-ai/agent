/**
 * Returns a stable callback that invalidates both the `nodes` and `links`
 * query caches, triggering a refetch for all nodes/links queries in the tree.
 *
 * Import this instead of reaching for `queryClient` directly so the key
 * names stay in one place.
 */
import { useCallback } from "react";
import { queryClient } from "@listo/ui-core";

export function useInvalidateGraph(): () => Promise<void> {
  return useCallback(
    () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: ["nodes"] }),
        queryClient.invalidateQueries({ queryKey: ["links"] }),
      ]).then(() => undefined),
    [],
  );
}
