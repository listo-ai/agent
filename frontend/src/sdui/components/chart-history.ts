/**
 * Pure helpers for the `chart.history` IR block and the auto-bucket
 * heuristic. Kept in its own file so Chart.tsx stays under the 400-line
 * cap from docs/design/CODE-LAYOUT.md.
 */
import type { ChartHistoryPreset } from "../types";

export const DEFAULT_HISTORY_PRESETS: ChartHistoryPreset[] = [
  { label: "5m", duration_ms: 5 * 60 * 1000 },
  { label: "1h", duration_ms: 60 * 60 * 1000 },
  { label: "6h", duration_ms: 6 * 60 * 60 * 1000 },
  { label: "24h", duration_ms: 24 * 60 * 60 * 1000 },
  { label: "7d", duration_ms: 7 * 24 * 60 * 60 * 1000 },
  { label: "all", duration_ms: null },
];

export type Range = { from: number; to: number };

/**
 * Resolve a `history.range_ms` into a rolling `{from, to}` window.
 * Snaps `to` to the nearest 10s so the React Query queryKey stays
 * stable across rapid re-renders — we don't want to refetch on every
 * render just because `Date.now()` ticked.
 */
export function resolveHistoryRange(
  rangeMs: number | null | undefined,
): Range | undefined {
  if (rangeMs == null || rangeMs <= 0) return undefined;
  const now = Date.now();
  const to = Math.floor(now / 10_000) * 10_000;
  return { from: to - rangeMs, to };
}

/**
 * `pageState[key]` override > authored > undefined. The zoom gesture
 * writes `{from, to}` into `pageState`; explicit `{from: null, to: null}`
 * clears back to the authored default.
 */
export function resolveRange(
  pageState: unknown,
  key: string,
  authored: Range | undefined,
): Range | undefined {
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

/** The preset label whose duration matches the current range, if any. */
export function matchActivePreset(
  range: Range | undefined,
  presets: ChartHistoryPreset[],
): string | null {
  if (!range) return null;
  const span = range.to - range.from;
  // Tolerance covers the 10s snap in resolveHistoryRange + small
  // clock drift between a button click and the subsequent render.
  const tol = 15_000;
  for (const p of presets) {
    if (p.duration_ms == null) {
      if (range.from <= tol) return p.label;
    } else if (Math.abs(span - p.duration_ms) <= tol) {
      return p.label;
    }
  }
  return null;
}

/**
 * Target ~200 buckets across the window; step up to a "nice" size
 * (1s, 5s, 10s, 30s, 1m, 5m, 15m, 1h, 6h, 1d). Returns `null` when the
 * window is small enough that raw rows fit under `rawThresholdMs` —
 * no point bucketing a 1-minute chart.
 */
export function pickAutoBucket(
  rangeMs: number,
  rawThresholdMs: number,
): number | null {
  if (!Number.isFinite(rangeMs) || rangeMs <= 0) return null;
  if (rangeMs <= rawThresholdMs) return null;
  const target = rangeMs / 200;
  const steps = [
    1_000,
    5_000,
    10_000,
    30_000,
    60_000,
    5 * 60_000,
    15 * 60_000,
    60 * 60_000,
    6 * 60 * 60_000,
    24 * 60 * 60_000,
  ];
  for (const s of steps) if (s >= target) return s;
  return steps[steps.length - 1] ?? null;
}
