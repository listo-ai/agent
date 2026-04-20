/**
 * Series → recharts-row shaping + tick formatters. Extracted from
 * Chart.tsx to keep that file under the 400-line cap.
 */
import type { ChartConfig } from "@/components/ui/chart";
import type { ChartSeries } from "../types";

// Shadcn CSS variables — `var(--chart-1)` etc. resolve to theme-aware
// palette colours, so the chart looks right in light and dark mode
// without hand-picking hex codes.
const PALETTE_KEYS = [
  "chart-1",
  "chart-2",
  "chart-3",
  "chart-4",
  "chart-5",
] as const;

export type ChartRow = { ts: number } & Record<string, number | null>;

export function buildChartData(series: ChartSeries[]): {
  data: ChartRow[];
  config: ChartConfig;
  seriesKeys: string[];
} {
  const seriesKeys: string[] = [];
  const config: ChartConfig = {};
  const byTs = new Map<number, ChartRow>();

  series.forEach((s, i) => {
    if (s.points.length === 0) return;
    const key = safeKey(s.label, i);
    seriesKeys.push(key);
    config[key] = {
      label: s.label,
      color: `hsl(var(--${PALETTE_KEYS[i % PALETTE_KEYS.length]}))`,
    };
    for (const [ts, v] of s.points) {
      let row = byTs.get(ts);
      if (!row) {
        row = { ts };
        byTs.set(ts, row);
      }
      row[key] = v;
    }
  });

  const data = [...byTs.values()].sort((a, b) => a.ts - b.ts);
  return { data, config, seriesKeys };
}

// Recharts `dataKey` must be a valid property name — squash anything
// exotic in `series.label` (spaces, dots, unicode) to a stable key.
function safeKey(label: string, idx: number): string {
  const cleaned = label.replace(/[^a-zA-Z0-9_]/g, "_");
  return cleaned.length > 0 ? `s_${cleaned}` : `s_${idx}`;
}

export function formatTimeTick(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function formatValueTick(v: number): string {
  if (!Number.isFinite(v)) return "";
  const abs = Math.abs(v);
  if (abs >= 1000) {
    return v.toLocaleString(undefined, { maximumFractionDigits: 0 });
  }
  if (abs >= 10) return v.toFixed(1);
  return v.toFixed(2);
}

export function coerceNumber(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (v === true) return 1;
  if (v === false) return 0;
  return null;
}
