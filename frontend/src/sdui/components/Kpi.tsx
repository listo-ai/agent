/**
 * `kpi` component — big-number stat tile.
 *
 * Reads the current value of `source.{node_id, slot}` by querying the
 * node once, then lets `useSubscriptions` patch the cache in place on
 * every `slot_changed` event the server emits against the kpi's
 * subscription plan. No telemetry / history round-trips — the current
 * live value is the only thing displayed.
 */
import { useQuery } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { extractField } from "../field";
import type { KpiNode } from "../types";

const INTENT_CLASSES: Record<string, string> = {
  ok: "text-emerald-500",
  warn: "text-amber-500",
  danger: "text-destructive",
  info: "text-sky-500",
};

export function KpiComponent({ node }: { node: KpiNode }) {
  const widgetId = node.id ?? `${node.source.node_id}.${node.source.slot}`;
  const { node_id, slot } = node.source;

  const query = useQuery<unknown>({
    queryKey: ["sdui-kpi", widgetId, node_id, slot],
    staleTime: 60_000,
    queryFn: async (): Promise<unknown> => {
      const client = await agentPromise;
      const resp = await client.nodes.getNodesPage({
        filter: `id=="${node_id}"`,
        size: 1,
      });
      const snap = resp.data[0];
      if (!snap) return null;
      const found = snap.slots.find((s) => s.name === slot);
      return found?.value ?? null;
    },
  });

  const value = extractField(query.data, node.source.field);
  const display = formatValue(value, node.format);

  const intentClass =
    node.intent && INTENT_CLASSES[node.intent]
      ? INTENT_CLASSES[node.intent]
      : "text-foreground";

  return (
    <div className="flex flex-col gap-1 rounded-lg border border-border bg-card px-4 py-3">
      <div className="text-xs uppercase tracking-wide text-muted-foreground">
        {node.label}
      </div>
      <div className={`text-3xl font-semibold tabular-nums ${intentClass}`}>
        {query.isLoading && query.data === undefined ? "—" : display}
      </div>
    </div>
  );
}

function formatValue(v: unknown, format?: string): string {
  if (v === null || v === undefined) return "—";
  if (typeof v === "boolean") return v ? "Yes" : "No";
  if (typeof v === "number") {
    switch (format) {
      case "percent":
        return `${(v * 100).toFixed(1)}%`;
      case "bytes":
        return formatBytes(v);
      default:
        return formatNumber(v);
    }
  }
  return String(v);
}

function formatNumber(n: number): string {
  if (!Number.isFinite(n)) return String(n);
  if (Math.abs(n) >= 1000) return n.toLocaleString();
  return n % 1 === 0 ? n.toString() : n.toFixed(2);
}

function formatBytes(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}
