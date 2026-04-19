/**
 * SSE subscription consumer for SDUI trees.
 *
 * Given the resolver's / render endpoint's `subscriptions: [{widget_id,
 * subjects}]` plan, this hook opens the agent's event stream and
 * invalidates the React Query cache for the current resolve / render
 * query whenever any matching `slot_changed` event arrives.  Subjects
 * follow the `node.<id>.slot.<name>` convention emitted by the backend
 * (see `SubscriptionPlan` in crates/dashboard-transport/src/resolve.rs).
 *
 * Wiring rule: components that live-update (`table`, slot-bound
 * `badge` / `text` / `kpi`) don't poll.  The resolver emits a plan
 * per-widget; this hook does the matching and the invalidation — the
 * components themselves stay dumb.
 */
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import type { UiSubscriptionPlan } from "@sys/agent-client";

/**
 * Subscribe to the agent event stream and invalidate the given
 * React-Query key whenever a slot-change event lands on a subject the
 * plan mentions.
 *
 * @param queryKey       the React-Query key to invalidate on match
 * @param subscriptions  the subscription plan returned by /ui/resolve
 *                       or /ui/render (may be empty)
 */
export function useSubscriptions(
  queryKey: readonly unknown[],
  subscriptions: UiSubscriptionPlan[] | undefined,
): void {
  const qc = useQueryClient();

  useEffect(() => {
    const plans = subscriptions ?? [];
    if (plans.length === 0) return;

    // Flatten into a set of `node.<id>.slot.<name>` subjects for O(1)
    // lookup. A single slot_changed event yields exactly one subject;
    // membership in the set means "invalidate".
    const subjectSet = new Set<string>();
    for (const p of plans) {
      for (const s of p.subjects) subjectSet.add(s);
    }

    let cancelled = false;
    let close: (() => void) | undefined;

    (async () => {
      const agent = await agentPromise;
      const sub = agent.events.subscribe();
      close = () => {
        // The transport's async iterator stops on cancel — the best
        // portable cancellation is to set the flag and break out.
        cancelled = true;
      };
      for await (const event of sub) {
        if (cancelled) break;
        if (event.event !== "slot_changed") continue;
        const subject = `node.${event.id}.slot.${event.slot}`;
        if (subjectSet.has(subject)) {
          // Invalidate both: (1) the owning resolve/render response
          // (in case the tree shape depends on the slot) and (2) every
          // sdui-table query, since tables fetch their rows via a
          // separate endpoint and their caches are keyed off the query
          // string — not the slot. A slot write on a node in a table's
          // result set must refetch that table.
          qc.invalidateQueries({ queryKey });
          qc.invalidateQueries({ queryKey: ["sdui-table"] });
        }
      }
    })();

    return () => {
      if (close) close();
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(queryKey), JSON.stringify(subscriptions)]);
}
