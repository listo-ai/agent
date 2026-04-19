import type { HistoryQueryOptions, HistoryRecord, ScalarRecord } from "@sys/agent-client";

import { useSlotHistory, useSlotTelemetry } from "@/hooks/useSlotHistory";
import { SlotHistoryTable } from "./SlotHistoryTable";
import { ScalarHistoryTable } from "./ScalarHistoryTable";
import { Skeleton } from "@/components/ui/skeleton";

interface SmartSlotHistoryPanelProps {
  nodePath: string;
  slot: string;
  /**
   * Current live value of the slot — used to pick the right backend store.
   * Bool / Number → telemetry; everything else → history.
   */
  currentValue: unknown;
  opts?: HistoryQueryOptions;
}

/**
 * Detects whether the slot routes to the telemetry or history store based on
 * the current live value, then fetches and renders the correct table.
 * No routing logic lives in the caller.
 */
export function SmartSlotHistoryPanel({
  nodePath,
  slot,
  currentValue,
  opts,
}: SmartSlotHistoryPanelProps) {
  const isScalar =
    typeof currentValue === "boolean" || typeof currentValue === "number";

  return isScalar ? (
    <TelemetryView nodePath={nodePath} slot={slot} {...(opts !== undefined && { opts })} />
  ) : (
    <HistoryView nodePath={nodePath} slot={slot} {...(opts !== undefined && { opts })} />
  );
}

// ─── inner views (each calls exactly one hook) ───────────────────────────────

function HistoryView({
  nodePath,
  slot,
  opts,
}: {
  nodePath: string;
  slot: string;
  opts?: HistoryQueryOptions;
}) {
  const { data, isLoading, isError, error } = useSlotHistory(nodePath, slot, opts);
  if (isLoading) return <LoadingRows />;
  if (isError) return <ErrorMsg error={error} />;
  return <SlotHistoryTable records={(data ?? []) as HistoryRecord[]} />;
}

function TelemetryView({
  nodePath,
  slot,
  opts,
}: {
  nodePath: string;
  slot: string;
  opts?: HistoryQueryOptions;
}) {
  const { data, isLoading, isError, error } = useSlotTelemetry(nodePath, slot, opts);
  if (isLoading) return <LoadingRows />;
  if (isError) return <ErrorMsg error={error} />;
  return <ScalarHistoryTable records={(data ?? []) as ScalarRecord[]} />;
}

function LoadingRows() {
  return (
    <div className="space-y-2 py-2">
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-full" />
      <Skeleton className="h-8 w-4/5" />
    </div>
  );
}

function ErrorMsg({ error }: { error: unknown }) {
  return (
    <p className="py-4 text-sm text-destructive">
      Failed to load:{" "}
      {error instanceof Error ? error.message : String(error)}
    </p>
  );
}
