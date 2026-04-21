/**
 * ScopeContext — makes the current FleetScope and its AgentClient available
 * throughout the subtree mounted under `/scope/:tenant/:agent_id/...`.
 *
 * Separation of concerns
 * ----------------------
 * - `ScopeProvider` is the one component that reads URL params (routing
 *   concern) and converts them into a `FleetScope` + `AgentClient` (domain
 *   concern). Everything else in this module is routing-unaware.
 * - `useScope()` and `useScopedAgent()` read from context only — no URL
 *   knowledge.
 * - The `fleetRequestFn` injection point is explicit: change the import in
 *   this file to swap the backend without touching any hook or component.
 */

import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { useParams } from "react-router-dom";
import { AgentClient, FleetScope } from "@listo/agent-client";
import { AGENT_BASE_URL } from "@/lib/agent";
import { fleetRequestStub } from "./fleetStub";
import type { ScopeContextValue, ScopeClientState } from "./types";

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const ScopeContext = createContext<ScopeContextValue | null>(null);

// ---------------------------------------------------------------------------
// ScopeProvider
// ---------------------------------------------------------------------------

interface ScopeProviderProps {
  children: ReactNode;
}

/**
 * Mount this at the `/scope/:tenant/:agent_id` layout route.
 *
 * Reads `:tenant` and `:agent_id` from the URL, constructs a
 * `FleetScope.remote(…)`, and asynchronously creates a `Remote`-scoped
 * `AgentClient` using the current `fleetRequestFn`.
 *
 * To swap from the stub to a real WS backend:
 *   - Change: `import { fleetRequestStub } from "./fleetStub"`
 *   - To:     `import { zenohFleetRequest } from "./zenohFleetRequest"` (or nats…)
 *   - Pass it below as `fleetRequestFn: zenohFleetRequest`.
 */
export function ScopeProvider({ children }: ScopeProviderProps) {
  const { tenant, agent_id } = useParams<{
    tenant: string;
    agent_id: string;
  }>();

  // Scope is always resolved synchronously — the route only mounts when
  // both params are present.
  const scope: FleetScope =
    tenant && agent_id
      ? FleetScope.remote(tenant, agent_id)
      : FleetScope.local();

  const [clientState, setClientState] = useState<ScopeClientState>({
    status: "loading",
    client: null,
    errorDetail: null,
  });

  // Re-create the AgentClient whenever the scope identity changes.
  const scopeKey = FleetScope.isRemote(scope)
    ? `${scope.tenant}/${scope.agent_id}`
    : "local";

  useEffect(() => {
    setClientState({ status: "loading", client: null, errorDetail: null });

    if (FleetScope.isLocal(scope)) {
      // Local scope — just use the singleton (no separate client needed).
      // This branch is hit when tenant/agent_id params are absent; should
      // not happen under normal routing but defend against it.
      void AgentClient.connect({
        baseUrl: AGENT_BASE_URL,
        skipCapabilityCheck: true,
      }).then((client) => {
        setClientState({ status: "ready", client, errorDetail: null });
      });
      return;
    }

    // Remote scope — fleet req/reply via the injected request fn.
    void AgentClient.connect({
      baseUrl: AGENT_BASE_URL, // baseUrl unused for remote, required by opts shape
      scope,
      fleetRequestFn: fleetRequestStub, // ← swap to real backend here
      skipCapabilityCheck: true, // capability check goes via fleet; skip for now
    })
      .then((client) => {
        setClientState({ status: "ready", client, errorDetail: null });
      })
      .catch((err: unknown) => {
        const message =
          err instanceof Error ? err.message : "Unknown error connecting to remote agent";
        setClientState({ status: "error", client: null, errorDetail: message });
      });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeKey]);

  return (
    <ScopeContext.Provider value={{ scope, clientState }}>
      {children}
    </ScopeContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Context accessor (private — only exported hooks use it)
// ---------------------------------------------------------------------------

/** Returns the current ScopeContext value. Returns null outside a scope route. */
export function useScopeContext(): ScopeContextValue | null {
  return useContext(ScopeContext);
}
