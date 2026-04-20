/**
 * `chart` component — inline SVG time-series viewer.
 *
 * Data source:
 * - If the author hard-codes `series`, render those points verbatim
 *   (useful for tests and snapshots).
 * - Otherwise fetch scalar telemetry for `source.{node_id, slot}` via
 *   the REST telemetry endpoint and map records into a single series.
 *   The useSubscriptions hook re-invalidates this query whenever the
 *   resolver-emitted plan says the slot ticked, so it stays live.
 *
 * Drag-to-zoom writes `{from, to}` into the page-state key the server
 * told us to use (defaults to `"chart_range"`); the round-trip to
 * `/ui/resolve` then re-emits series with the focused window (once
 * server-side population ships) — until then the range is used to
 * clamp the client-side fetch window.
 */
import { useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import type { ChartNode, ChartSeries } from "../types";
import { useSdui } from "../context";

const PAD = 4;
const W = 600;
const H = 160;
const DEFAULT_WINDOW_MS = 60 * 60 * 1000; // last hour
const DEFAULT_LIMIT = 500;

export function ChartComponent({ node }: { node: ChartNode }) {
  const { setPageState } = useSdui();
  const stateKey = node.page_state_key ?? "chart_range";
  const svgRef = useRef<SVGSVGElement | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);

  const authoredSeries = node.series ?? [];
  const hasAuthored = authoredSeries.some((s) => s.points.length > 0);

  const fetched = useChartFetch(node, hasAuthored);
  const series = hasAuthored ? authoredSeries : fetched.series;

  const bounds = computeBounds(series, node.range);
  if (!bounds) {
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

  function clientXToTs(clientX: number): number {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect || !bounds) return 0;
    const xFrac = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    return Math.round(bounds.tMin + xFrac * (bounds.tMax - bounds.tMin));
  }

  return (
    <svg
      ref={svgRef}
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label={`chart ${node.id ?? ""}`}
      className="w-full select-none rounded border bg-card"
      onMouseDown={(e) => setDragFrom(clientXToTs(e.clientX))}
      onMouseMove={(e) => {
        if (dragFrom !== null) setDragTo(clientXToTs(e.clientX));
      }}
      onMouseUp={() => {
        if (dragFrom !== null && dragTo !== null && Math.abs(dragTo - dragFrom) > 0) {
          const [from, to] = dragFrom < dragTo ? [dragFrom, dragTo] : [dragTo, dragFrom];
          setPageState({ [stateKey]: { from, to } });
        }
        setDragFrom(null);
        setDragTo(null);
      }}
      onMouseLeave={() => {
        setDragFrom(null);
        setDragTo(null);
      }}
      onDoubleClick={() => {
        setPageState({ [stateKey]: null });
      }}
    >
      {series.map((s, i) => (
        <SeriesPath key={s.label + i} series={s} bounds={bounds} idx={i} />
      ))}
      {dragFrom !== null && dragTo !== null ? (
        <ZoomOverlay from={dragFrom} to={dragTo} bounds={bounds} />
      ) : null}
    </svg>
  );
}

function useChartFetch(
  node: ChartNode,
  hasAuthored: boolean,
): { series: ChartSeries[]; isLoading: boolean; isError: boolean } {
  const { node_id, slot } = node.source;
  const range = node.range;
  const widgetId = node.id ?? `${node_id}.${slot}`;
  const from = range?.from;
  const to = range?.to;

  // node_id → path is cached long-term; path only changes on rename.
  // Splitting it off keeps per-tick invalidations from re-resolving
  // the same id every time.
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

  // Remember which store actually has data so later invalidations
  // only hit one endpoint. First fetch probes both; subsequent fetches
  // skip the one known to be empty for this (node, slot).
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

      // Pin the store on the first probe that yielded rows.
      if (sourceRef.current === "both") {
        if (telemetry.length > 0) sourceRef.current = "telemetry";
        else if (history.length > 0) sourceRef.current = "history";
      }

      const points: [number, number][] = [];
      for (const r of telemetry) {
        const v = coerceNumber(r.value);
        if (v !== null) points.push([r.ts_ms, v]);
      }
      if (points.length === 0) {
        for (const r of history) {
          const v = coerceNumber(extractPayload(r.value));
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

function coerceNumber(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (v === true) return 1;
  if (v === false) return 0;
  return null;
}

/**
 * Flow-engine msg envelopes carry the real value at `.payload`. If the
 * slot value is a bare scalar, pass it through unchanged.
 */
function extractPayload(v: unknown): unknown {
  if (v && typeof v === "object" && !Array.isArray(v) && "payload" in v) {
    return (v as { payload: unknown }).payload;
  }
  return v;
}

type Bounds = { tMin: number; tMax: number; vMin: number; vMax: number };

function computeBounds(
  series: ChartSeries[],
  range: ChartNode["range"],
): Bounds | null {
  let tMin = Infinity;
  let tMax = -Infinity;
  let vMin = Infinity;
  let vMax = -Infinity;
  for (const s of series) {
    for (const [t, v] of s.points) {
      if (t < tMin) tMin = t;
      if (t > tMax) tMax = t;
      if (v < vMin) vMin = v;
      if (v > vMax) vMax = v;
    }
  }
  if (range) {
    tMin = range.from;
    tMax = range.to;
  }
  if (!isFinite(tMin) || !isFinite(tMax) || tMin === tMax) return null;
  if (!isFinite(vMin) || !isFinite(vMax)) return null;
  if (vMin === vMax) {
    vMin -= 1;
    vMax += 1;
  }
  return { tMin, tMax, vMin, vMax };
}

function SeriesPath({
  series,
  bounds,
  idx,
}: {
  series: ChartSeries;
  bounds: Bounds;
  idx: number;
}) {
  if (series.points.length === 0) return null;
  const d = series.points
    .map(([t, v], i) => {
      const x = PAD + ((t - bounds.tMin) / (bounds.tMax - bounds.tMin)) * (W - PAD * 2);
      const y =
        H - PAD - ((v - bounds.vMin) / (bounds.vMax - bounds.vMin)) * (H - PAD * 2);
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  const palette = ["currentColor", "#8b5cf6", "#ef4444", "#10b981"];
  return (
    <path
      d={d}
      fill="none"
      stroke={palette[idx % palette.length]}
      strokeWidth="1.5"
      vectorEffect="non-scaling-stroke"
    />
  );
}

function ZoomOverlay({
  from,
  to,
  bounds,
}: {
  from: number;
  to: number;
  bounds: Bounds;
}) {
  const [lo, hi] = from < to ? [from, to] : [to, from];
  const x1 = PAD + ((lo - bounds.tMin) / (bounds.tMax - bounds.tMin)) * (W - PAD * 2);
  const x2 = PAD + ((hi - bounds.tMin) / (bounds.tMax - bounds.tMin)) * (W - PAD * 2);
  return (
    <rect
      x={x1}
      y={0}
      width={Math.max(1, x2 - x1)}
      height={H}
      fill="currentColor"
      opacity="0.08"
    />
  );
}
