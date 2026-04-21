// ---------------------------------------------------------------------------
// Fleet lib — shared types
//
// FleetScope is re-exported from the client package so nothing in the
// frontend ever imports the client schema directly. All fleet logic
// operates on these types; components receive them as props / hook results.
// ---------------------------------------------------------------------------

import type { FleetScope } from "@listo/agent-client";

export type { FleetScope };

// ---------------------------------------------------------------------------
// RemoteAgentNode — the frontend domain type for a sys.fleet.remote-agent
// graph node.
//
// Mapped from a raw NodeSnapshot so UI never depends on graph schema details.
// ---------------------------------------------------------------------------

export type RemoteAgentConnection =
  | "connected"
  | "reconnecting"
  | "disconnected"
  | "unknown";

export interface RemoteAgentNode {
  /** Graph node id. */
  id: string;
  /** Graph path, e.g. `/agent/fleet/edge-42`. */
  path: string;
  /** Display name from the `display_name` slot (falls back to `agentId`). */
  displayName: string;
  /** Tenant from the `tenant` slot. */
  tenant: string;
  /** Agent id from the `agent_id` slot. */
  agentId: string;
  /** Live connection status from the `connection` slot. */
  connection: RemoteAgentConnection;
  /** ISO-8601 string from `last_seen`, or null. */
  lastSeen: string | null;
  /** Scope object derived from tenant + agentId. */
  scope: FleetScope;
  /** Route to navigate to when this node is selected. */
  scopeUrl: string;
}

// ---------------------------------------------------------------------------
// ScopeClientState — the async lifecycle of a scoped AgentClient.
//
// Components that need the remote client read this from `useScopedAgent()`.
// Mirrors the three-state pattern used throughout the codebase
// (loading / ready / error).
// ---------------------------------------------------------------------------

export type ScopeClientStatus = "loading" | "ready" | "error";

export interface ScopeClientState {
  status: ScopeClientStatus;
  /** Present when `status === "ready"`. */
  client: import("@listo/agent-client").AgentClient | null;
  /** Present when `status === "error"`. */
  errorDetail: string | null;
}

// ---------------------------------------------------------------------------
// ScopeContextValue — what ScopeContext holds.
//
// `scope` is always resolved synchronously from the URL.
// `clientState` is async — starts "loading", resolves to ready/error.
// ---------------------------------------------------------------------------

export interface ScopeContextValue {
  scope: FleetScope;
  clientState: ScopeClientState;
}
