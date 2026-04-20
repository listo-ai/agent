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
import { useEffect } from "react";
import { useQueryClient, type Query } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
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

  useEffect(() => {
    const plans = subscriptions ?? [];
    if (plans.length === 0) return;

    const subjectToWidget = new Map<string, string>();
    for (const p of plans) {
      for (const s of p.subjects) subjectToWidget.set(s, p.widget_id);
    }

    let cancelled = false;

    (async () => {
      const agent = await agentPromise;
      const sub = agent.events.subscribe();
      for await (const event of sub) {
        if (cancelled) break;
        if (event.event !== "slot_changed") continue;
        const subject = `node.${event.id}.slot.${event.slot}`;
        const widget = subjectToWidget.get(subject);
        if (!widget) continue;

        if (UUID_RE.test(widget)) {
          // Tree-binding plan — `{{$target.<slot>}}` was substituted at
          // resolve time, so the tree itself must re-resolve.
          qc.invalidateQueries({ queryKey });
          continue;
        }

        // Component-scoped plans: patch the cache in place. Touches
        // every matching query (different pageState / pagination)
        // via setQueriesData.
        applyToTables(qc, widget, event);
        applyToCharts(qc, widget, event);
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(queryKey), JSON.stringify(subscriptions)]);
}

function applyToTables(
  qc: ReturnType<typeof useQueryClient>,
  widget: string,
  event: SlotChangedEvent,
): void {
  qc.setQueriesData<UiTableResponse>(
    { predicate: matchesPrefix(["sdui-table", widget]) },
    (old) => {
      if (!old) return old;
      let touched = false;
      const data = old.data.map((row: UiTableRow) => {
        if (row.id !== event.id) return row;
        touched = true;
        return { ...row, slots: { ...row.slots, [event.slot]: event.value } };
      });
      return touched ? { ...old, data } : old;
    },
  );
}

function applyToCharts(
  qc: ReturnType<typeof useQueryClient>,
  widget: string,
  event: SlotChangedEvent,
): void {
  const v = coerceNumber(extractPayload(event.value));
  if (v === null) return;
  qc.setQueriesData<ChartSeries[]>(
    { predicate: matchesPrefix(["sdui-chart", widget]) },
    (old) => {
      if (!old || old.length === 0) return old;
      // Append to the first series. Charts currently render exactly
      // one source; when multi-series lands, the event includes slot
      // and we'd pick by label.
      const [head, ...rest] = old;
      if (!head) return old;
      const nextPoints: [number, number][] = [
        ...head.points,
        [event.ts, v],
      ];
      return [{ label: head.label, points: nextPoints }, ...rest];
    },
  );
}

function matchesPrefix(prefix: readonly unknown[]) {
  return (q: Query): boolean => {
    const key = q.queryKey as readonly unknown[];
    if (key.length < prefix.length) return false;
    for (let i = 0; i < prefix.length; i++) {
      if (key[i] !== prefix[i]) return false;
    }
    return true;
  };
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
