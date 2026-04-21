import type { HistoryQueryOptions } from "@listo/agent-client";

import { useSlotHistory } from "@/hooks/useSlotHistory";
import { SlotHistoryTable } from "./SlotHistoryTable";
import { Skeleton } from "@/components/ui/skeleton";

interface SlotHistoryPanelProps {
  /** Full node path, e.g. `/heartbeat`. */
  nodePath: string;
  /** Slot name, e.g. `notes`. */
  slot: string;
  opts?: HistoryQueryOptions;
}

/**
 * Smart shell: fetches history via `useSlotHistory` and renders the table
 * together with loading and error states. Drop this anywhere you need an
 * inline history view.
 *
 * Intentionally has no record button — compose `RecordHistoryButton` next
 * to it when you need that action.
 */
export function SlotHistoryPanel({ nodePath, slot, opts }: SlotHistoryPanelProps) {
  const { data, isLoading, isError, error } = useSlotHistory(nodePath, slot, opts);

  if (isLoading) {
    return (
      <div className="space-y-2 py-2">
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-full" />
        <Skeleton className="h-8 w-4/5" />
      </div>
    );
  }

  if (isError) {
    return (
      <p className="py-4 text-sm text-destructive">
        Failed to load history:{" "}
        {error instanceof Error ? error.message : String(error)}
      </p>
    );
  }

  return <SlotHistoryTable records={data ?? []} />;
}
