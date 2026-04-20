/**
 * `date_range` component — row of preset buttons that write a
 * `{from, to}` window (Unix ms) into `$page[page_state_key]`.
 *
 * Consumers (typically a `chart`) read the same key and retune. A
 * preset with `duration_ms: null` means "all time"; the value written
 * is `{from: null, to: null}` so consumers can drop their clamps.
 *
 * The first preset is applied on mount when the key is unset — gives
 * authors a sane default without extra wiring.
 */
import { useEffect } from "react";
import { useSdui } from "../context";
import type { DateRangeNode, DateRangePresetShape } from "../types";

interface Range {
  from: number | null;
  to: number | null;
}

function presetToRange(p: DateRangePresetShape): Range {
  if (p.duration_ms == null) return { from: null, to: null };
  const to = Date.now();
  return { from: to - p.duration_ms, to };
}

function isRange(v: unknown): v is Range {
  return (
    typeof v === "object" &&
    v !== null &&
    "from" in v &&
    "to" in v &&
    (typeof (v as Range).from === "number" || (v as Range).from === null) &&
    (typeof (v as Range).to === "number" || (v as Range).to === null)
  );
}

function rangeMatchesPreset(r: Range, p: DateRangePresetShape, tolMs = 5_000): boolean {
  if (p.duration_ms == null) return r.from === null && r.to === null;
  if (r.from === null || r.to === null) return false;
  // "to" slides with wall-clock — match by duration, tolerating a
  // few seconds of drift so the active chip stays highlighted.
  const duration = r.to - r.from;
  return Math.abs(duration - p.duration_ms) <= tolMs;
}

export function DateRangePicker({ node }: { node: DateRangeNode }) {
  const { pageState, setPageState } = useSdui();
  const key = node.page_state_key;
  const current = (pageState as Record<string, unknown>)[key];
  const currentRange: Range | null = isRange(current) ? current : null;

  const firstPreset = node.presets[0];

  // Default to the first preset when no range is pinned yet.
  useEffect(() => {
    if (currentRange === null && firstPreset) {
      setPageState({ [key]: presetToRange(firstPreset) });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (node.presets.length === 0) return null;

  return (
    <div role="group" aria-label="Time range" className="flex flex-wrap gap-1">
      {node.presets.map((p) => {
        const active = currentRange !== null && rangeMatchesPreset(currentRange, p);
        return (
          <button
            key={p.label + (p.duration_ms ?? "all")}
            type="button"
            aria-pressed={active}
            onClick={() => setPageState({ [key]: presetToRange(p) })}
            className={
              "rounded-md border px-2.5 py-1 text-xs transition-colors " +
              (active
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border text-muted-foreground hover:bg-accent hover:text-foreground")
            }
          >
            {p.label}
          </button>
        );
      })}
    </div>
  );
}
