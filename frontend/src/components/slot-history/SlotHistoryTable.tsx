import { useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";
import type { HistoryRecord } from "@listo/agent-client";

import { cn } from "@/lib/utils";

interface SlotHistoryTableProps {
  records: HistoryRecord[];
}

function formatTs(tsMs: number): string {
  const d = new Date(tsMs);
  return d.toLocaleString(undefined, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/** Returns true when the value deserves an expandable JSON view. */
function isExpandable(v: unknown): boolean {
  return v !== null && typeof v === "object";
}

/** One-line preview — always compact. */
function valueSummary(record: HistoryRecord): string {
  if (record.slot_kind === "binary") return `(binary · ${record.byte_size} B)`;
  const v = record.value;
  if (v === null || v === undefined) return "—";
  if (typeof v === "string") return v;
  if (typeof v === "boolean" || typeof v === "number") return String(v);
  if (Array.isArray(v)) return `[${v.length} item${v.length === 1 ? "" : "s"}]`;
  const keys = Object.keys(v as object);
  const preview = keys
    .slice(0, 3)
    .map((k) => `${k}: …`)
    .join(", ");
  return `{ ${preview}${keys.length > 3 ? `, +${keys.length - 3} more` : ""} }`;
}

function JsonView({ value }: { value: unknown }) {
  return (
    <pre className="mt-2 overflow-auto rounded-md border border-border bg-muted/50 p-3 font-mono text-[11px] leading-relaxed text-foreground">
      {JSON.stringify(value, null, 2)}
    </pre>
  );
}

function HistoryRow({ record }: { record: HistoryRecord }) {
  const [expanded, setExpanded] = useState(false);
  const expandable = isExpandable(record.value) && record.slot_kind !== "binary";

  return (
    <tr
      className={cn(
        "border-b border-border/50 transition-colors",
        expandable && "cursor-pointer hover:bg-accent/40",
        expanded && "bg-accent/20",
      )}
      onClick={expandable ? () => setExpanded((v) => !v) : undefined}
    >
      {/* Timestamp */}
      <td className="py-2 pl-1 pr-3 align-top font-mono text-[11px] text-muted-foreground whitespace-nowrap">
        {formatTs(record.ts_ms)}
      </td>

      {/* Value */}
      <td className="py-2 pr-1 align-top">
        <div className="flex items-start gap-1">
          {expandable ? (
            <span className="mt-0.5 shrink-0 text-muted-foreground">
              {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            </span>
          ) : (
            <span className="mt-0.5 w-3 shrink-0" />
          )}
          <span
            className={cn(
              "font-mono text-[11px] break-all",
              expandable && !expanded && "text-muted-foreground italic",
            )}
          >
            {valueSummary(record)}
          </span>
        </div>
        {expanded && <JsonView value={record.value} />}
      </td>
    </tr>
  );
}

/**
 * Pure presentational table. Click any object/array row to expand the full JSON.
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
    <table className="w-full text-sm">
      <thead>
        <tr className="border-b border-border">
          <th className="pb-2 pr-3 text-left text-xs font-medium text-muted-foreground whitespace-nowrap">
            Timestamp
          </th>
          <th className="pb-2 text-left text-xs font-medium text-muted-foreground">
            Value
          </th>
        </tr>
      </thead>
      <tbody>
        {records.map((r) => (
          <HistoryRow key={r.id} record={r} />
        ))}
      </tbody>
    </table>
  );
}
