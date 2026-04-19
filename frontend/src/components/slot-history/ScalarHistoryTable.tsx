import type { ScalarRecord } from "@sys/agent-client";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";

interface ScalarHistoryTableProps {
  records: ScalarRecord[];
}

function formatTs(tsMs: number): string {
  return new Date(tsMs).toLocaleString(undefined, {
    dateStyle: "short",
    timeStyle: "medium",
  });
}

/**
 * Pure presentational table for Bool / Number scalar history records.
 * No data-fetching, no side-effects.
 */
export function ScalarHistoryTable({ records }: ScalarHistoryTableProps) {
  if (records.length === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">
        No telemetry records yet.
      </p>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-[180px]">Timestamp</TableHead>
          <TableHead>Value</TableHead>
          <TableHead className="w-[80px]">Type</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {records.map((r, i) => {
          const isBoolean = typeof r.value === "boolean";
          return (
            <TableRow key={i}>
              <TableCell className="font-mono text-xs text-muted-foreground">
                {formatTs(r.ts_ms)}
              </TableCell>
              <TableCell className="font-mono text-xs">
                {r.value === null ? "—" : String(r.value)}
              </TableCell>
              <TableCell>
                <Badge variant="secondary" className="text-xs">
                  {isBoolean ? "bool" : "number"}
                </Badge>
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
