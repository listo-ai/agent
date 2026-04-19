import type { HistoryRecord } from "@sys/agent-client";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";

interface SlotHistoryTableProps {
  records: HistoryRecord[];
}

function formatTs(tsMs: number): string {
  return new Date(tsMs).toLocaleString(undefined, {
    dateStyle: "short",
    timeStyle: "medium",
  });
}

function formatValue(record: HistoryRecord): string {
  if (record.slot_kind === "binary") return "(binary)";
  if (record.value === null || record.value === undefined) return "—";
  if (typeof record.value === "string") return record.value;
  return JSON.stringify(record.value);
}

/**
 * Pure presentational table. Receives an already-fetched `records` array and
 * renders it. Has no data-fetching, no side-effects, no hooks beyond React.
 */
export function SlotHistoryTable({ records }: SlotHistoryTableProps) {
  if (records.length === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">
        No history records yet.
      </p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-[180px]">Timestamp</TableHead>
          <TableHead>Value</TableHead>
          <TableHead className="w-[80px]">Kind</TableHead>
          <TableHead className="w-[70px] text-right">Bytes</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {records.map((r) => (
          <TableRow key={r.id}>
            <TableCell className="font-mono text-xs text-muted-foreground">
              {formatTs(r.ts_ms)}
            </TableCell>
            <TableCell className="max-w-[320px] truncate font-mono text-xs">
              {formatValue(r)}
            </TableCell>
            <TableCell>
              <Badge variant="secondary" className="text-xs">
                {r.slot_kind}
              </Badge>
            </TableCell>
            <TableCell className="text-right text-xs text-muted-foreground">
              {r.byte_size}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
