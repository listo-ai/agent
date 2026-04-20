/**
 * `select` component — a native dropdown that writes the chosen
 * option's `value` into `$page[page_state_key]`. Downstream widgets
 * (tables via `{{$page.<key>}}` in `source.query`, charts reading the
 * same key, etc.) retune whenever the select changes.
 *
 * Uses native `<select>` for now — zero dependency, accessible out of
 * the box. Swap for a searchable combobox when a concrete use case
 * asks for it.
 */
import { useEffect } from "react";
import { useSdui } from "../context";
import type { SelectNode, SelectOptionShape } from "../types";

export function SelectComponent({ node }: { node: SelectNode }) {
  const { pageState, setPageState } = useSdui();
  const key = node.page_state_key;
  const current = (pageState as Record<string, unknown>)[key];

  // Apply `default` on mount when the key is unset. The author
  // chooses whether `default` maps to one of `options[].value` — we
  // write it verbatim and let downstream validate.
  useEffect(() => {
    if (current === undefined && node.default !== undefined) {
      setPageState({ [key]: node.default });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const currentIdx = findOptionIndex(node.options, current);

  return (
    <select
      value={currentIdx >= 0 ? String(currentIdx) : ""}
      onChange={(e) => {
        const idx = Number(e.target.value);
        const opt = node.options[idx];
        if (opt) setPageState({ [key]: opt.value });
      }}
      className="rounded-md border border-border bg-background px-2 py-1 text-xs"
    >
      {node.placeholder !== undefined && currentIdx < 0 && (
        <option value="" disabled>
          {node.placeholder}
        </option>
      )}
      {node.options.map((opt, i) => (
        <option key={i} value={String(i)}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

/**
 * Options carry JSON values — compare structurally so complex values
 * still highlight. Index-based so we don't need a stable stringify.
 */
function findOptionIndex(options: SelectOptionShape[], current: unknown): number {
  if (current === undefined) return -1;
  for (let i = 0; i < options.length; i++) {
    if (deepEqual(options[i]?.value, current)) return i;
  }
  return -1;
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (a && b && typeof a === "object") {
    return JSON.stringify(a) === JSON.stringify(b);
  }
  return false;
}
