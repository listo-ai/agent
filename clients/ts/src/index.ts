// Narrow public surface — re-export only what consumers need.
// Nothing else in this package is considered public API.

export { AgentClient } from "./client.js";
export type { AgentClientOptions } from "./client.js";

export type { ClientError, HttpError, CapabilityMismatchError, ParseError, TimeoutError } from "./errors.js";
export { asClientError } from "./errors.js";

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
  Lifecycle,
} from "./schemas/events.js";
export type { NodeSnapshot, Slot } from "./schemas/node.js";
export type { Link, LinkEndpoint } from "./schemas/link.js";
export type { PluginSummary, PluginLifecycle } from "./schemas/plugin.js";
export type {
  CapabilityManifest,
  Capability,
  PlatformInfo,
  ApiInfo,
} from "./schemas/capability.js";

// Domain request shapes.
export type { LinkEndpointRef } from "./domain/links.js";
export type { SeedPreset, SeedResult } from "./domain/seed.js";
export type { NodeConfig } from "./domain/config.js";
