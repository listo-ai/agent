import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { HistoryRecord, HistoryQueryOptions, ScalarRecord } from "@listo/agent-client";

import { useAgent } from "@listo/ui-core";

// ---- query keys ------------------------------------------------------------

/** Stable query key factory so invalidation is always exact-match safe. */
export const historyKeys = {
  history: (path: string, slot: string, opts?: HistoryQueryOptions) =>
    ["slot-history", path, slot, opts ?? {}] as const,
  telemetry: (path: string, slot: string, opts?: HistoryQueryOptions) =>
    ["slot-telemetry", path, slot, opts ?? {}] as const,
};

// ---- useSlotHistory --------------------------------------------------------

/**
 * Fetches recorded history for a **String / Json / Binary** slot.
 *
 * Disabled when `path` or `slot` are empty strings.
 * Obeys the app-wide 30 s staleTime; data is never re-fetched on the
 * background unless `refetch()` is called explicitly (e.g. after recording).
 */
export function useSlotHistory(
  path: string,
  slot: string,
  opts?: HistoryQueryOptions,
) {
  const agentQuery = useAgent();
  return useQuery<HistoryRecord[]>({
    queryKey: historyKeys.history(path, slot, opts),
    queryFn: () => agentQuery.data!.history.listHistory(path, slot, opts),
    enabled: path.length > 0 && slot.length > 0 && agentQuery.data !== undefined,
  });
}

// ---- useSlotTelemetry ------------------------------------------------------

/**
 * Fetches recorded scalar series for a **Bool / Number** slot.
 *
 * Same enabling and staleness rules as `useSlotHistory`.
 */
export function useSlotTelemetry(
  path: string,
  slot: string,
  opts?: HistoryQueryOptions,
) {
  const agentQuery = useAgent();
  return useQuery<ScalarRecord[]>({
    queryKey: historyKeys.telemetry(path, slot, opts),
    queryFn: () => agentQuery.data!.history.listTelemetry(path, slot, opts),
    enabled: path.length > 0 && slot.length > 0 && agentQuery.data !== undefined,
  });
}

// ---- useRecordHistory ------------------------------------------------------

/**
 * On-demand snapshot mutation.
 *
 * Calling `record()` POSTs to `POST /api/v1/history/record` and, on
 * success, invalidates **both** the matching history and telemetry queries
 * so any open `useSlotHistory` / `useSlotTelemetry` panels refetch
 * automatically. The caller never has to think about cache invalidation.
 */
export function useRecordHistory(path: string, slot: string) {
  const agentQuery = useAgent();
  const qc = useQueryClient();

  const mutation = useMutation({
    mutationFn: () => agentQuery.data!.history.record(path, slot),
    onSuccess: () => {
      // Invalidate all history/telemetry entries for this node+slot regardless
      // of time-range opts — the key prefix match handles every variant.
      qc.invalidateQueries({ queryKey: ["slot-history", path, slot] });
      qc.invalidateQueries({ queryKey: ["slot-telemetry", path, slot] });
    },
  });

  return {
    /** Call to snapshot the current live slot value into the history store. */
    record: mutation.mutateAsync,
    isPending: mutation.isPending,
    error: mutation.error,
    isSuccess: mutation.isSuccess,
  };
}
