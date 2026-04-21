/**
 * useScope — returns the current FleetScope derived from the URL.
 *
 * Outside a `/scope/:tenant/:agent_id/...` route this returns
 * `FleetScope.local()`. Inside a scope route it returns
 * `FleetScope.remote(tenant, agent_id)`.
 *
 * No URL knowledge here — the scope is resolved by `ScopeProvider`
 * from the URL params and stored in context. Hooks and components read
 * this hook, never `useParams()` directly.
 */

import { FleetScope } from "@sys/agent-client";
import { useScopeContext } from "./ScopeContext";
import type { FleetScope as FleetScopeType } from "./types";

/**
 * Returns the active `FleetScope`.
 *
 * `FleetScope.local()` — talking to this agent over HTTP (default).
 * `FleetScope.remote(tenant, agent_id)` — routing calls through fleet.
 */
export function useScope(): FleetScopeType {
  const ctx = useScopeContext();
  return ctx?.scope ?? FleetScope.local();
}
