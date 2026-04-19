// Narrow public surface — re-export only what consumers need.
// Nothing else in this package is considered public API.

export { AgentClient } from "./client.js";
export type { AgentClientOptions } from "./client.js";

// Transport abstraction — consumers that wire their own fleet backend
// need these to implement `fleetRequestFn` and `RequestTransport`.
export type { RequestTransport, FleetRequestFn } from "./transport/request.js";
export { FleetRequestTransport } from "./transport/fleet_request.js";
export { pathToSubject } from "./transport/request.js";

export type { ClientError, HttpError, CapabilityMismatchError, ParseError, TimeoutError } from "./errors.js";
export { asClientError, GenerationMismatchError } from "./errors.js";

export { CLIENT_VERSION, REST_API_VERSION, REQUIRED_CAPABILITIES } from "./version.js";
export type { CapabilityRequirement } from "./version.js";

// Schema types for callers that need raw wire shapes.
export type { Msg } from "./schemas/msg.js";
export type {
  GraphEvent,
  NodeCreatedEvent,
  NodeRemovedEvent,
  NodeRenamedEvent,
  SlotChangedEvent,
  LifecycleTransitionEvent,
  LinkAddedEvent,
  LinkRemovedEvent,
  LinkBrokenEvent,
  HelloEvent,
  Lifecycle,
} from "./schemas/events.js";
export type { NodeSnapshot, NodeListResponse, PageMeta, Slot } from "./schemas/node.js";
export type { Link, LinkEndpoint } from "./schemas/link.js";
export type { Kind, Facet, SlotDefinition, SlotRole } from "./schemas/kind.js";
export type {
  PluginSummary,
  PluginLifecycle,
  PluginRuntimeState,
  PluginRuntimeEntry,
} from "./schemas/plugin.js";
export type { WhoAmI, Scope } from "./schemas/auth.js";
// FleetScope is both a const namespace (constructors/guards) and a discriminated
// union type — a single export covers both because TS tracks value and type
// declarations under the same identifier.
export { FleetScope } from "./schemas/fleet.js";
export type {
  CapabilityManifest,
  Capability,
  PlatformInfo,
  ApiInfo,
} from "./schemas/capability.js";

export type {
  UiNavNode,
  UiComponent,
  UiComponentTree,
  UiResolveMeta,
  UiResolveIssue,
  UiResolveRequest,
  UiResolveResponse,
  UiSubscriptionPlan,
  UiAction,
  UiTableSource,
  UiTableColumn,
  UiDiffAnnotation,
  // S2
  UiActionContext,
  UiActionRequest,
  UiActionResponse,
  UiNavigateTo,
  // S3
  UiTableParams,
  UiTableRow,
  UiTableMeta,
  UiTableResponse,
} from "./schemas/ui.js";
export type { WriteSlotResponse } from "./schemas/slot.js";
export type { SeedResult, SeededNode } from "./schemas/seed.js";
export type { HistoryRecord, ScalarRecord, RecordResult } from "./schemas/history.js";
export type { HistoryApi, HistoryQueryOptions } from "./domain/history.js";

// Domain request shapes.
export type { LinkEndpointRef } from "./domain/links.js";
export type { SeedPreset } from "./domain/seed.js";
export type { NodeConfig } from "./domain/config.js";
