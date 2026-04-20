/**
 * `chart` component — recharts line chart with shadcn theming.
 *
 * Data source:
 * - Authored `series` (hard-coded points) renders verbatim.
 * - Otherwise fetch scalar telemetry/history for `source.{node_id, slot}`
 *   and the `useSubscriptions` hook patches the query cache live.
 *
 * Drag-to-zoom: hold + drag across the plot; on mouseup we write
 * `{from, to}` into `$page[page_state_key]` (default `"chart_range"`).
 * Double-click clears the range.
 *
 * The internal series shape stays `{ label, points: [ts, value][] }` so
 * the SSE patch path in `useSubscriptions.ts` doesn't change — we adapt
 * to recharts' record shape at render time.
 */
import { useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  CartesianGrid,
  Line,
  LineChart,
  ReferenceArea,
  XAxis,
  YAxis,
} from "recharts";
import { agentPromise } from "@/lib/agent";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import type { ChartNode, ChartSeries } from "../types";
import { useSdui } from "../context";
import { extractField } from "../field";

const DEFAULT_WINDOW_MS = 60 * 60 * 1000;
const DEFAULT_LIMIT = 500;

// Shadcn's chart CSS variables — `var(--chart-1)` etc. resolve to
// theme-aware palette colours, so the same component looks right in
// both light and dark mode without hand-picking hex codes.
const PALETTE_KEYS = ["chart-1", "chart-2", "chart-3", "chart-4", "chart-5"] as const;

export function ChartComponent({ node }: { node: ChartNode }) {
  const { pageState, setPageState } = useSdui();
  const stateKey = node.page_state_key ?? "chart_range";
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);

  const authoredSeries = node.series ?? [];
  const hasAuthored = authoredSeries.some((s) => s.points.length > 0);

  const effectiveRange = resolveRange(pageState, stateKey, node.range);
  const fetched = useChartFetch(node, hasAuthored, effectiveRange);
  const series = hasAuthored ? authoredSeries : fetched.series;

  const { data, config, seriesKeys } = useMemo(
    () => buildChartData(series),
    [series],
  );

  if (data.length === 0) {
    return (
      <div className="rounded border border-dashed border-muted-foreground/30 px-3 py-6 text-center text-xs text-muted-foreground">
        {fetched.isLoading
          ? "Loading…"
          : fetched.isError
          ? "Failed to load telemetry."
          : "No data in the visible window."}
      </div>
    );
  }

  function commitZoom() {
    if (dragFrom !== null && dragTo !== null && dragFrom !== dragTo) {
      const [from, to] =
        dragFrom < dragTo ? [dragFrom, dragTo] : [dragTo, dragFrom];
      setPageState({ [stateKey]: { from, to } });
    }
    setDragFrom(null);
    setDragTo(null);
  }

  return (
    <ChartContainer config={config} className="aspect-auto h-[200px] w-full">
      <LineChart
        data={data}
        margin={{ top: 8, right: 12, bottom: 0, left: 8 }}
        onMouseDown={(e) => {
          if (e?.activeLabel !== undefined) setDragFrom(Number(e.activeLabel));
        }}
        onMouseMove={(e) => {
          if (dragFrom !== null && e?.activeLabel !== undefined) {
            setDragTo(Number(e.activeLabel));
          }
        }}
        onMouseUp={commitZoom}
        onMouseLeave={() => {
          setDragFrom(null);
          setDragTo(null);
        }}
        onDoubleClick={() => setPageState({ [stateKey]: null })}
      >
        <CartesianGrid vertical={false} strokeDasharray="3 3" />
        <XAxis
          dataKey="ts"
          type="number"
          domain={
            effectiveRange
              ? [effectiveRange.from, effectiveRange.to]
              : ["dataMin", "dataMax"]
          }
          scale="time"
          tickLine={false}
          axisLine={false}
          tickMargin={6}
          minTickGap={40}
          tickFormatter={formatTimeTick}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          tickMargin={6}
          width={36}
          tickFormatter={formatValueTick}
        />
        <ChartTooltip
          cursor={{ strokeDasharray: "3 3" }}
          content={
            <ChartTooltipContent
              labelFormatter={(ts) =>
                typeof ts === "number" ? new Date(ts).toLocaleString() : String(ts)
              }
            />
          }
        />
        {seriesKeys.map((key) => (
          <Line
            key={key}
            dataKey={key}
            type="monotone"
            stroke={`var(--color-${key})`}
            strokeWidth={2}
            dot={false}
            isAnimationActive={false}
            connectNulls
          />
        ))}
        {dragFrom !== null && dragTo !== null && dragFrom !== dragTo ? (
          <ReferenceArea
            x1={Math.min(dragFrom, dragTo)}
            x2={Math.max(dragFrom, dragTo)}
            strokeOpacity={0}
            fillOpacity={0.12}
          />
        ) : null}
      </LineChart>
    </ChartContainer>
  );
}

type ChartRow = { ts: number } & Record<string, number | null>;

function buildChartData(series: ChartSeries[]): {
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

function formatTimeTick(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatValueTick(v: number): string {
  if (!Number.isFinite(v)) return "";
  const abs = Math.abs(v);
  if (abs >= 1000) return v.toLocaleString(undefined, { maximumFractionDigits: 0 });
  if (abs >= 10) return v.toFixed(1);
  return v.toFixed(2);
}

function useChartFetch(
  node: ChartNode,
  hasAuthored: boolean,
  range: { from: number; to: number } | undefined,
): { series: ChartSeries[]; isLoading: boolean; isError: boolean } {
  const { node_id, slot, field } = node.source;
  const widgetId = node.id ?? `${node_id}.${slot}`;
  const from = range?.from;
  const to = range?.to;

  const pathQuery = useQuery({
    queryKey: ["sdui-chart-path", node_id],
    enabled: !hasAuthored && !!node_id,
    staleTime: 60_000,
    queryFn: async (): Promise<string | null> => {
      const client = await agentPromise;
      const resp = await client.nodes.getNodesPage({
        filter: `id=="${node_id}"`,
        size: 1,
      });
      return resp.data[0]?.path ?? null;
    },
  });

  const sourceRef = useRef<"telemetry" | "history" | "both">("both");
  const path = pathQuery.data ?? null;

  const query = useQuery({
    queryKey: ["sdui-chart", widgetId, node_id, slot, from ?? null, to ?? null],
    enabled: !hasAuthored && !!slot && !!path,
    staleTime: 5_000,
    queryFn: async (): Promise<ChartSeries[]> => {
      if (!path) return [{ label: slot, points: [] }];
      const client = await agentPromise;
      const effectiveTo = to ?? Date.now();
      const effectiveFrom = from ?? effectiveTo - DEFAULT_WINDOW_MS;
      const opts = {
        from: effectiveFrom,
        to: effectiveTo,
        limit: DEFAULT_LIMIT,
      };

      const want = sourceRef.current;
      const [telemetry, history] = await Promise.all([
        want === "history"
          ? Promise.resolve([])
          : client.history.listTelemetry(path, slot, opts).catch(() => []),
        want === "telemetry"
          ? Promise.resolve([])
          : client.history.listHistory(path, slot, opts).catch(() => []),
      ]);

      if (sourceRef.current === "both") {
        if (telemetry.length > 0) sourceRef.current = "telemetry";
        else if (history.length > 0) sourceRef.current = "history";
      }

      const points: [number, number][] = [];
      for (const r of telemetry) {
        const v = coerceNumber(field ? extractField(r.value, field) : r.value);
        if (v !== null) points.push([r.ts_ms, v]);
      }
      if (points.length === 0) {
        for (const r of history) {
          const v = coerceNumber(extractField(r.value, field));
          if (v !== null) points.push([r.ts_ms, v]);
        }
      }
      return [{ label: slot, points }];
    },
  });

  return {
    series: query.data ?? [],
    isLoading: pathQuery.isLoading || query.isLoading,
    isError: pathQuery.isError || query.isError,
  };
}

function resolveRange(
  pageState: unknown,
  key: string,
  authored: { from: number; to: number } | undefined,
): { from: number; to: number } | undefined {
  const state =
    pageState && typeof pageState === "object"
      ? (pageState as Record<string, unknown>)[key]
      : undefined;
  if (
    state &&
    typeof state === "object" &&
    state !== null &&
    "from" in state &&
    "to" in state
  ) {
    const s = state as { from: unknown; to: unknown };
    if (typeof s.from === "number" && typeof s.to === "number") {
      return { from: s.from, to: s.to };
    }
    if (s.from === null && s.to === null) return undefined;
  }
  return authored;
}

function coerceNumber(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (v === true) return 1;
  if (v === false) return 0;
  return null;
}

