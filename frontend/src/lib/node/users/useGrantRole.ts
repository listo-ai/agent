import { useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useAgent } from "@listo/ui-core";
import { formatError } from "@/lib/utils";
import type { GrantRoleState } from "./types";

/**
 * Mutation hook for `POST /api/v1/users/{id}/grants`.
 *
 * Returns 202 Accepted. The actual Zitadel fan-out is backend-deferred;
 * the UI gets instant feedback via `status`.
 *
 * On success, the users list query is automatically invalidated so the
 * table refreshes without a manual reload.
 */
export function useGrantRole(): GrantRoleState {
  const { data: client } = useAgent();
  const queryClient = useQueryClient();
  const [status, setStatus] = useState<GrantRoleState["status"]>("idle");
  const [errorDetail, setErrorDetail] = useState<string | null>(null);

  const grant = useCallback(
    async (userId: string, role: string, bulkActionId: string) => {
      if (!client) return;
      setStatus("pending");
      setErrorDetail(null);
      try {
        await client.users.grantRole(userId, { role, bulk_action_id: bulkActionId });
        setStatus("ok");
        // Invalidate so any open UsersTable re-fetches.
        void queryClient.invalidateQueries({ queryKey: ["users"] });
      } catch (err) {
        setStatus("error");
        setErrorDetail(formatError(err));
      }
    },
    [client, queryClient],
  );

  const reset = useCallback(() => {
    setStatus("idle");
    setErrorDetail(null);
  }, []);

  return { status, errorDetail, grant, reset };
}
