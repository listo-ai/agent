/**
 * `sparkline` component — compact inline line chart for KPI tiles.
 *
 * No axes, no interaction. Pure visual summary of a recent value
 * series. Live-update via the subscription plan — `subscribe` should
 * match a `node.<id>.slot.<slot>` subject the server emitted.
 */
import type { SparklineNode } from "../types";

const W = 120;
const H = 32;
const PAD = 2;

export function SparklineComponent({ node }: { node: SparklineNode }) {
  const values = node.values ?? [];
  if (values.length < 2) {
    return (
      <svg viewBox={`0 0 ${W} ${H}`} className="h-6 w-24 opacity-50" aria-hidden />
    );
  }
  let vMin = Infinity;
  let vMax = -Infinity;
  for (const v of values) {
    if (v < vMin) vMin = v;
    if (v > vMax) vMax = v;
  }
  if (vMin === vMax) {
    vMin -= 1;
    vMax += 1;
  }
  const d = values
    .map((v, i) => {
      const x = PAD + (i / (values.length - 1)) * (W - PAD * 2);
      const y = H - PAD - ((v - vMin) / (vMax - vMin)) * (H - PAD * 2);
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");
  const intentClass: Record<string, string> = {
    ok: "text-emerald-600",
    warn: "text-amber-600",
    danger: "text-rose-600",
  };
  const colour = intentClass[node.intent ?? ""] ?? "text-muted-foreground";
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className={`h-6 w-24 ${colour}`}
      role="img"
      aria-label={`sparkline ${node.id ?? ""}`}
    >
      <path
        d={d}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
