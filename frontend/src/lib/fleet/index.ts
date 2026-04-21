// ---------------------------------------------------------------------------
// lib/fleet — fleet scope logic for Studio.
//
// Pure logic (hooks + types) and lightweight UI components are kept
// together here following the same pattern as lib/nav. Routing concerns
// are isolated inside ScopeProvider (the single component that reads
// URL params). Everything else is context, hooks, and components that
// any part of the tree can consume without knowing about routing.
//
// Public surface
// --------------

// Types
export type {
  FleetScope,
  RemoteAgentNode,
  RemoteAgentConnection,
  ScopeClientState,
  ScopeClientStatus,
  ScopeContextValue,
} from "./types";

// Provider (used once, at the router layout level)
export { ScopeProvider } from "./ScopeContext";

// Hooks — logic only, no UI
export { useScope } from "./useScope";
export { useScopedAgent } from "./useScopedAgent";
export { useRemoteAgents } from "./useRemoteAgents";

// UI components
export { RemoteAgentsSection } from "./RemoteAgentsSection";
export type { RemoteAgentsSectionProps } from "./RemoteAgentsSection";
export { ScopeIndicator } from "./ScopeIndicator";

// Stub + error (exported so tests and future backends can import the shape)
export { fleetRequestStub, FleetNotConnectedError } from "./fleetStub";
