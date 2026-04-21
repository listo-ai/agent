/**
 * useScopedAgent — returns the `AgentClient` instance for the current scope.
 *
 * Outside a `/scope/...` route this always returns `{ status: "loading" }`
 * until the context is populated, or the caller can use `useAgent()` for
 * the local singleton.
 *
 * Inside a `/scope/:tenant/:agent_id/...` route this returns the in-progress
 * or completed remote-scoped `AgentClient`.
 *
 * Usage
 * -----
 * This hook is for components that explicitly need the remote-scoped client
 * (e.g. the remote-agent subtree renderer). Components that should always
 * talk to the local agent use `useAgent()` from `@/hooks/useAgent`.
 */

import { useScopeContext } from "./ScopeContext";
import type { ScopeClientState } from "./types";

/**
 * Returns the async state of the scoped `AgentClient`.
 *
 * `status === "loading"` — client is being constructed (async `connect`).
 * `status === "ready"`   — `client` is non-null and ready to use.
 * `status === "error"`   — construction failed; `errorDetail` has the reason.
 *
 * Returns a permanent `{ status: "loading" }` sentinel when called outside
 * a `ScopeProvider` (i.e. outside a `/scope/...` route). This is intentional
 * — components that render unconditionally will show their loading state
 * rather than crash.
 */
export function useScopedAgent(): ScopeClientState {
  const ctx = useScopeContext();
  if (!ctx) {
    // Not inside a ScopeProvider — return a stable "not connected" state.
    // Used when a component that may appear in both local and scoped contexts
    // calls this hook; the local context simply never becomes ready here.
    return { status: "loading", client: null, errorDetail: null };
  }
  return ctx.clientState;
}
