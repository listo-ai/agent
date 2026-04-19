/**
 * `chart` component — inline SVG time-series viewer.
 *
 * The server fills `series` with `[ts_ms, value]` points and an
 * optional `range` covering the visible window. Drag-to-zoom writes
 * `{from, to}` into the page-state key the server told us to use
 * (defaults to `"chart_range"`); the round-trip to `/ui/resolve` then
 * returns denser data for the focused window. No client-side
 * aggregation — the server owns density.
 *
 * Deliberately no dependency: the SDUI renderer's size budget keeps
 * first-class vocabulary light. Swap for recharts / uplot when a
 * concrete UC needs tooltips, axes, or multiple series.
 */
import { useRef, useState } from "react";
import type { ChartNode, ChartSeries } from "../types";
import { useSdui } from "../context";

const PAD = 4;
const W = 600;
const H = 160;

export function ChartComponent({ node }: { node: ChartNode }) {
  const { setPageState } = useSdui();
  const stateKey = node.page_state_key ?? "chart_range";
  const svgRef = useRef<SVGSVGElement | null>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);

  const bounds = computeBounds(node.series, node.range);
  if (!bounds) {
    return (
      <div className="rounded border border-dashed border-muted-foreground/30 px-3 py-6 text-center text-xs text-muted-foreground">
        No data in the visible window.
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
        // Clear zoom → resolve returns the default window.
        setPageState({ [stateKey]: null });
      }}
    >
      {node.series.map((s, i) => (
        <SeriesPath key={s.label + i} series={s} bounds={bounds} idx={i} />
      ))}
      {dragFrom !== null && dragTo !== null ? (
        <ZoomOverlay from={dragFrom} to={dragTo} bounds={bounds} />
      ) : null}
    </svg>
  );
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
