import type { ScalarRecord } from "@listo/agent-client";

interface ScalarHistoryTableProps {
  records: ScalarRecord[];
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

function formatValue(v: unknown): string {
  if (v === null || v === undefined) return "—";
  if (typeof v === "boolean") return v ? "true" : "false";
  if (typeof v === "number") {
    // Show up to 6 sig figs without trailing zeros
    return Number.isInteger(v) ? String(v) : v.toPrecision(6).replace(/\.?0+$/, "");
  }
  return String(v);
}

/**
 * Pure presentational table for Bool / Number scalar telemetry records.
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
    <table className="w-full text-sm">
      <thead>
        <tr className="border-b border-border">
          <th className="pb-2 pr-3 text-left text-xs font-medium text-muted-foreground whitespace-nowrap">
            Timestamp
          </th>
          <th className="pb-2 text-left text-xs font-medium text-muted-foreground">
            Value
          </th>
          <th className="pb-2 pl-3 text-right text-xs font-medium text-muted-foreground">
            Type
          </th>
        </tr>
      </thead>
      <tbody>
        {records.map((r, i) => (
          <tr key={i} className="border-b border-border/50">
            <td className="py-2 pr-3 font-mono text-[11px] text-muted-foreground whitespace-nowrap">
              {formatTs(r.ts_ms)}
            </td>
            <td className="py-2 font-mono text-[11px]">
              {formatValue(r.value)}
            </td>
            <td className="py-2 pl-3 text-right font-mono text-[11px] text-muted-foreground">
              {typeof r.value === "boolean" ? "bool" : "num"}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
