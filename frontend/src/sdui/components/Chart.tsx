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
} from "@listo/ui-kit";
import type { ChartHistoryPreset, ChartNode, ChartSeries } from "../types";
import { useSdui } from "../context";
import { extractField } from "../field";
import {
  DEFAULT_HISTORY_PRESETS,
  matchActivePreset,
  pickAutoBucket,
  resolveHistoryRange,
  resolveRange,
} from "./chart-history";
import {
  buildChartData,
  coerceNumber,
  formatTimeTick,
  formatValueTick,
} from "./chart-data";

const DEFAULT_WINDOW_MS = 60 * 60 * 1000;
const DEFAULT_LIMIT = 500;

export function ChartComponent({ node }: { node: ChartNode }) {
  const { pageState, setPageState } = useSdui();
  const stateKey = node.page_state_key ?? "chart_range";
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);

  const authoredSeries = node.series ?? [];
  const hasAuthored = authoredSeries.some((s) => s.points.length > 0);

  // `history.range_ms` rolls from "now at mount". Snapping to a 10s
  // bucket keeps the queryKey stable across renders; SSE appends
  // forward from there, so fine-grained drift isn't needed.
  const historyRange = useMemo(
    () => resolveHistoryRange(node.history?.range_ms),
    // range_ms is a primitive; recompute when the IR changes.
    [node.history?.range_ms],
  );

  const effectiveRange = resolveRange(
    pageState,
    stateKey,
    node.range ?? historyRange,
  );
  const fetched = useChartFetch(node, hasAuthored, effectiveRange);
  const series = hasAuthored ? authoredSeries : fetched.series;

  const presets = node.history?.user_selectable
    ? node.history.presets && node.history.presets.length > 0
      ? node.history.presets
      : DEFAULT_HISTORY_PRESETS
    : null;

  const activePresetLabel = presets
    ? matchActivePreset(effectiveRange, presets)
    : null;

  const { data, config, seriesKeys } = useMemo(
    () => buildChartData(series),
    [series],
  );

  function commitZoom() {
    if (dragFrom !== null && dragTo !== null && dragFrom !== dragTo) {
      const [from, to] =
        dragFrom < dragTo ? [dragFrom, dragTo] : [dragTo, dragFrom];
      setPageState({ [stateKey]: { from, to } });
    }
    setDragFrom(null);
    setDragTo(null);
  }

  function applyPreset(p: ChartHistoryPreset) {
    const to = Date.now();
    const from = p.duration_ms == null ? 0 : to - p.duration_ms;
    setPageState({ [stateKey]: { from, to } });
  }

  const picker = presets ? (
    <div className="flex flex-wrap gap-1 pb-2 text-xs">
      {presets.map((p) => {
        const active = p.label === activePresetLabel;
        return (
          <button
            key={p.label}
            type="button"
            onClick={() => applyPreset(p)}
            className={
              "rounded border px-2 py-0.5 transition-colors " +
              (active
                ? "border-foreground bg-foreground text-background"
                : "border-muted-foreground/30 text-muted-foreground hover:bg-muted")
            }
          >
            {p.label}
          </button>
        );
      })}
    </div>
  ) : null;

  if (data.length === 0) {
    const historyRequested =
      node.history?.range_ms != null && node.history.range_ms > 0;
    return (
      <div>
        {picker}
        <div className="rounded border border-dashed border-muted-foreground/30 px-3 py-6 text-center text-xs text-muted-foreground">
          {fetched.isLoading ? (
            "Loading…"
          ) : fetched.isError ? (
            "Failed to load telemetry."
          ) : historyRequested ? (
            <HistoryEmptyHint node={node} />
          ) : (
            "No data in the visible window."
          )}
        </div>
      </div>
    );
  }

  return (
    <div>
      {picker}
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
    </div>
  );
}

/**
 * Shown when a chart with `history.range_ms` set gets zero points back.
 * The SSE stream can still be delivering live ticks fine — what's
 * missing is *recorded* data in the agent's history/telemetry store.
 * This almost always means the source slot has no
 * `sys.core.history.config` policy, so the historizer never wrote
 * anything. Tell the user that directly instead of "no data".
 */
function HistoryEmptyHint({ node }: { node: ChartNode }) {
  const { node_id, slot, field } = node.source;
  return (
    <div className="space-y-1 text-left">
      <div className="font-medium text-foreground">
        No recorded history for this slot yet.
      </div>
      <div>
        The chart backfill reads from the agent's history/telemetry store.
        Configure a historization policy so values start getting recorded.
      </div>
      <div className="text-muted-foreground/70">
        node <code>{node_id.slice(0, 8)}…</code> · slot <code>{slot}</code>
        {field ? (
          <>
            {" "}
            · field <code>{field}</code>
          </>
        ) : null}
      </div>
    </div>
  );
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

  // History policies with `paths:` store each under `<slot>.<field>`
  // (see `effective_path_name` in domain-history/historizer.rs).
  const telemetrySlot = field ? `${slot}.${field}` : slot;

  // Auto-bucket once the window is large enough that raw rows would
  // blow past DEFAULT_LIMIT. Below the threshold we keep today's raw
  // fetch (no visual change); above it we downsample server-side.
  const effectiveTo = to ?? Date.now();
  const effectiveFrom = from ?? effectiveTo - DEFAULT_WINDOW_MS;
  const bucketMs = pickAutoBucket(
    effectiveTo - effectiveFrom,
    DEFAULT_LIMIT * 1000,
  );

  const query = useQuery({
    queryKey: [
      "sdui-chart",
      widgetId,
      node_id,
      slot,
      from ?? null,
      to ?? null,
      bucketMs ?? null,
    ],
    enabled: !hasAuthored && !!slot && !!path,
    staleTime: 5_000,
    queryFn: async (): Promise<ChartSeries[]> => {
      if (!path) return [{ label: slot, points: [] }];
      const client = await agentPromise;
      const opts = {
        from: effectiveFrom,
        to: effectiveTo,
        limit: DEFAULT_LIMIT,
      };

      if (bucketMs && sourceRef.current !== "history") {
        // Bucketed path — server aggregates, so the client just
        // stitches `(ts_ms, value)` pairs into the series shape.
        // Use the expanded slot name so we find historizer-expanded
        // per-path samples, not the un-recorded parent slot.
        try {
          const resp = await client.history.listTelemetryBucketed(
            path,
            telemetrySlot,
            { ...opts, bucket: bucketMs, agg: "avg" },
          );
          if (resp.data.length > 0) {
            sourceRef.current = "telemetry";
            const points: [number, number][] = [];
            for (const r of resp.data) {
              if (r.value !== null && Number.isFinite(r.value)) {
                points.push([r.ts_ms, r.value]);
              }
            }
            return [{ label: slot, points }];
          }
        } catch {
          // Fall through to raw path — bucketed endpoint failure
          // shouldn't black out an otherwise-working chart.
        }
      }

      const want = sourceRef.current;
      const [telemetry, history] = await Promise.all([
        want === "history"
          ? Promise.resolve([])
          : client.history
              .listTelemetry(path, telemetrySlot, opts)
              .catch(() => []),
        want === "telemetry"
          ? Promise.resolve([])
          : client.history.listHistory(path, slot, opts).catch(() => []),
      ]);

      if (sourceRef.current === "both") {
        if (telemetry.length > 0) sourceRef.current = "telemetry";
        else if (history.length > 0) sourceRef.current = "history";
      }

      const points: [number, number][] = [];
      // We query the expanded slot name when `field` is set, so the
      // telemetry value is already the extracted scalar. Only
      // fallback-extract if the row came back under the raw slot.
      for (const r of telemetry) {
        const v = coerceNumber(r.value);
        if (v !== null) points.push([r.ts_ms, v]);
      }
      if (points.length === 0) {
        // Raw history store holds the whole JSON envelope — extract
        // the declared `field` to land on the numeric payload.
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

