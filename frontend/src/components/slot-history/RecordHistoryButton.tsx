import { RotateCcw } from "lucide-react";

import { useRecordHistory } from "@/hooks/useSlotHistory";
import { Button } from "@listo/ui-kit";
import { cn } from "@/lib/utils";

interface RecordHistoryButtonProps {
  nodePath: string;
  slot: string;
  className?: string;
}

/**
 * Single-responsibility button that calls `POST /api/v1/history/record`
 * for the given node + slot and automatically refreshes any open history
 * panels via query invalidation.
 */
export function RecordHistoryButton({
  nodePath,
  slot,
  className,
}: RecordHistoryButtonProps) {
  const { record, isPending } = useRecordHistory(nodePath, slot);

  return (
    <Button
      variant="outline"
      size="sm"
      disabled={isPending}
      onClick={() => record()}
      className={cn("gap-1.5", className)}
    >
      <RotateCcw className={cn("size-3.5", isPending && "animate-spin")} />
      {isPending ? "Recording…" : "Record now"}
    </Button>
  );
}
