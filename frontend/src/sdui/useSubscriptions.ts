/**
 * SSE subscription consumer for SDUI trees.
 *
 * Given the resolver's / render endpoint's
 * `subscriptions: [{widget_id, subjects}]` plan, this hook opens the
 * agent's event stream and invalidates the right React-Query entry
 * whenever a matching `slot_changed` event arrives.
 *
 * Routing rule (see `crates/dashboard-transport/src/render.rs` —
 * `derive_subscriptions`):
 *
 * - `widget_id` is a UUID matching the target node → the template
 *   contains `{{$target.<slot>}}` references whose values are baked
 *   into the tree. Invalidate the owning resolve / render query so the
 *   tree re-resolves with the new slot values.
 * - `widget_id` is an authored IR component id (e.g. `"t"`,
 *   `"alarms"`) → the plan came from a `table` with `subscribe: true`.
 *   Invalidate only that table's query key
 *   (`["sdui-table", widget_id, …]`) — sibling tables stay untouched.
 */
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import type { UiSubscriptionPlan } from "@sys/agent-client";

// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function useSubscriptions(
  queryKey: readonly unknown[],
  subscriptions: UiSubscriptionPlan[] | undefined,
): void {
  const qc = useQueryClient();

  useEffect(() => {
    const plans = subscriptions ?? [];
    if (plans.length === 0) return;

    // Subject → widget_id index. Many subjects can map to the same
    // plan; one subject maps to exactly one plan (plans don't overlap
    // by construction in the backend).
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
          // Tree-binding plan — the target's own slot changed, the
          // whole tree must re-resolve.
          qc.invalidateQueries({ queryKey });
        } else {
          // Table plan — invalidate just that table's rows. The key
          // is a prefix match; React Query invalidates every query
          // whose key starts with this array.
          qc.invalidateQueries({ queryKey: ["sdui-table", widget] });
        }
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(queryKey), JSON.stringify(subscriptions)]);
}
