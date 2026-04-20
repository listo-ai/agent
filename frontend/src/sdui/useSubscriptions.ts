/**
 * SSE subscription consumer for SDUI trees.
 *
 * Given the resolver's / render endpoint's
 * `subscriptions: [{widget_id, subjects}]` plan, this hook opens the
 * agent's event stream and applies `slot_changed` events directly to
 * the matching React-Query caches — no refetch per tick.
 *
 * Two routing rules (see `crates/dashboard-transport/src/render.rs` —
 * `derive_subscriptions`):
 *
 * - `widget_id` is a UUID matching the target node → the template
 *   contains `{{$target.<slot>}}` references baked into the tree.
 *   Invalidate the owning resolve / render query so the tree
 *   re-resolves with the new slot values. (Tree can't be patched
 *   scalar-by-scalar; it has to re-run substitution.)
 * - `widget_id` is an authored IR component id (`"t"`, `"hb-chart"`) →
 *   apply the event directly:
 *     • tables   → merge the slot value into the matching row in-place
 *     • charts   → append `[ts, coerce(value)]` to the series
 *   Neither path hits the network. If the widget type isn't recognised
 *   (future component) we fall back to invalidating its prefix so the
 *   server remains the source of truth.
 */
import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { extractField } from "./field";
import type {
  SlotChangedEvent,
  UiSubscriptionPlan,
  UiTableResponse,
  UiTableRow,
} from "@sys/agent-client";

// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

type ChartSeries = { label: string; points: [number, number][] };

export function useSubscriptions(
  queryKey: readonly unknown[],
  subscriptions: UiSubscriptionPlan[] | undefined,
): void {
  const qc = useQueryClient();

  // The stream must be opened once and kept open while this consumer
  // is mounted. Earlier revisions keyed the effect on queryKey +
  // subscriptions content, which meant every edit (debouncedText
  // changes) or page-state change tore down the EventSource mid-tick
  // — and the cleanup never actually called `sub.close()`, so stale
  // connections leaked until the iterator happened to observe the
  // `cancelled` flag on its *next* event. Refs decouple the stream
  // lifecycle from render-driven inputs.
  const queryKeyRef = useRef(queryKey);
  const subjectsRef = useRef<Map<string, string[]>>(new Map());
  // widget_id -> field dot-path (populated for chart/kpi plans that
  // set `source.field`). Live-tick patch paths consult this to apply
  // the same extraction the initial fetch applies.
  const fieldsRef = useRef<Map<string, string | undefined>>(new Map());

  queryKeyRef.current = queryKey;

  useEffect(() => {
    const plans = subscriptions ?? [];
    const next = new Map<string, string[]>();
    const nextFields = new Map<string, string | undefined>();
    for (const p of plans) {
      for (const s of p.subjects) {
        const existing = next.get(s);
        if (existing) {
          if (!existing.includes(p.widget_id)) existing.push(p.widget_id);
        } else {
          next.set(s, [p.widget_id]);
        }
      }
      nextFields.set(p.widget_id, p.field);
    }
    subjectsRef.current = next;
    fieldsRef.current = nextFields;
    if (
      typeof window !== "undefined" &&
      window.localStorage?.getItem("sdui_debug") === "1"
    ) {
      console.debug("[sdui] subscription plan updated", {
        plans,
        subjects: [...next.entries()],
      });
    }
  }, [JSON.stringify(subscriptions)]);

  useEffect(() => {
    const debug =
      typeof window !== "undefined" &&
      window.localStorage?.getItem("sdui_debug") === "1";

    let cancelled = false;
    let sub: Awaited<ReturnType<typeof openStream>> | undefined;

    async function openStream() {
      const agent = await agentPromise;
      return agent.events.subscribe();
    }

    (async () => {
      sub = await openStream();
      if (cancelled) {
        sub.close();
        return;
      }
      if (debug) console.debug("[sdui] subscriptions mounted");
      for await (const event of sub) {
        if (cancelled) break;
        if (event.event !== "slot_changed") continue;
        const subject = `node.${event.id}.slot.${event.slot}`;
        const widgets = subjectsRef.current.get(subject);
        if (!widgets || widgets.length === 0) {
          if (debug) console.debug("[sdui] event ignored (no plan)", subject);
          continue;
        }
        if (debug) console.debug("[sdui] event matched", subject, "→", widgets);

        for (const widget of widgets) {
          if (UUID_RE.test(widget)) {
            qc.invalidateQueries({ queryKey: queryKeyRef.current });
            continue;
          }

          const field = fieldsRef.current.get(widget);
          const patched =
            applyToTables(qc, widget, event) ||
            applyToCharts(qc, widget, event, field) ||
            applyToKpis(qc, widget, event);
          if (!patched) {
            qc.invalidateQueries({ queryKey: ["sdui-table", widget] });
            qc.invalidateQueries({ queryKey: ["sdui-chart", widget] });
            qc.invalidateQueries({ queryKey: ["sdui-kpi", widget] });
          }
        }
      }
    })();

    return () => {
      cancelled = true;
      sub?.close();
    };
  }, [qc]);
}

function applyToTables(
  qc: ReturnType<typeof useQueryClient>,
  widget: string,
  event: SlotChangedEvent,
): boolean {
  let touchedAny = false;
  qc.setQueriesData<UiTableResponse>(
    { queryKey: ["sdui-table", widget] },
    (old) => {
      if (!old) return old;
      let touched = false;
      const data = old.data.map((row: UiTableRow) => {
        if (row.id !== event.id) return row;
        touched = true;
        return { ...row, slots: { ...row.slots, [event.slot]: event.value } };
      });
      if (touched) touchedAny = true;
      return touched ? { ...old, data } : old;
    },
  );
  return touchedAny;
}

function applyToKpis(
  qc: ReturnType<typeof useQueryClient>,
  widget: string,
  event: SlotChangedEvent,
): boolean {
  let touchedAny = false;
  qc.setQueriesData<unknown>(
    { queryKey: ["sdui-kpi", widget] },
    (_old: unknown) => {
      touchedAny = true;
      // KPI stores the raw slot value; the component unwraps
      // `.payload` at render time.
      return event.value;
    },
  );
  return touchedAny;
}

function applyToCharts(
  qc: ReturnType<typeof useQueryClient>,
  widget: string,
  event: SlotChangedEvent,
  field: string | undefined,
): boolean {
  const extracted = field
    ? extractField(event.value, field)
    : extractPayload(event.value);
  const v = coerceNumber(extracted);
  if (v === null) return false;
  let touchedAny = false;
  qc.setQueriesData<ChartSeries[]>(
    { queryKey: ["sdui-chart", widget] },
    (old) => {
      if (!old || old.length === 0) return old;
      const [head, ...rest] = old;
      if (!head) return old;
      touchedAny = true;
      return [
        { label: head.label, points: [...head.points, [event.ts, v]] },
        ...rest,
      ];
    },
  );
  return touchedAny;
}

function coerceNumber(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (v === true) return 1;
  if (v === false) return 0;
  return null;
}

function extractPayload(v: unknown): unknown {
  if (v && typeof v === "object" && !Array.isArray(v) && "payload" in v) {
    return (v as { payload: unknown }).payload;
  }
  return v;
}
