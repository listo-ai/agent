// Narrow public surface — re-export only what consumers need.
// Nothing else in this package is considered public API.

export { AgentClient } from "./client.js";
export type { AgentClientOptions } from "./client.js";

export type { ClientError, HttpError, CapabilityMismatchError, ParseError, TimeoutError } from "./errors.js";
export { asClientError } from "./errors.js";

export { CLIENT_VERSION, REST_API_VERSION, REQUIRED_CAPABILITIES } from "./version.js";
export type { CapabilityRequirement } from "./version.js";

// Schema types for callers that need to work with raw shapes.
export type { Msg } from "./schemas/msg.js";
export type { GraphEvent, SlotChangedEvent, NodeAddedEvent, NodeRemovedEvent } from "./schemas/events.js";
export type { NodeSnapshot, Slot } from "./schemas/node.js";
export type { CapabilityManifest, Capability } from "./schemas/capability.js";
