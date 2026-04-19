import { z } from "zod";

/**
 * Discriminated union of graph events emitted over SSE.
 *
 * Wire shape mirrors the Rust `graph::GraphEvent` enum
 * (`#[serde(tag = "event", rename_all = "snake_case")]`) — see
 * `crates/graph/src/event.rs`. Tag field is `event`, values are
 * snake_case.
 */

const SlotRefSchema = z.object({
  node: z.string(),
  slot: z.string(),
});

export const NodeCreatedEventSchema = z.object({
  event: z.literal("node_created"),
  id: z.string(),
  kind: z.string(),
  path: z.string(),
});

export const NodeRemovedEventSchema = z.object({
  event: z.literal("node_removed"),
  id: z.string(),
  kind: z.string(),
  path: z.string(),
});

export const NodeRenamedEventSchema = z.object({
  event: z.literal("node_renamed"),
  id: z.string(),
  old_path: z.string(),
  new_path: z.string(),
});

export const SlotChangedEventSchema = z.object({
  event: z.literal("slot_changed"),
  id: z.string(),
  path: z.string(),
  slot: z.string(),
  value: z.unknown(),
  generation: z.number().int().nonnegative(),
});

/** Matches the Rust `Lifecycle` enum (snake_case). */
export const LifecycleSchema = z.enum([
  "created",
  "active",
  "disabled",
  "stale",
  "fault",
  "removing",
  "removed",
]);
export type Lifecycle = z.infer<typeof LifecycleSchema>;

export const LifecycleTransitionEventSchema = z.object({
  event: z.literal("lifecycle_transition"),
  id: z.string(),
  path: z.string(),
  from: LifecycleSchema,
  to: LifecycleSchema,
});

export const LinkAddedEventSchema = z.object({
  event: z.literal("link_added"),
  id: z.string(),
  source: SlotRefSchema,
  target: SlotRefSchema,
});

export const LinkRemovedEventSchema = z.object({
  event: z.literal("link_removed"),
  id: z.string(),
  source: SlotRefSchema,
  target: SlotRefSchema,
});

export const LinkBrokenEventSchema = z.object({
  event: z.literal("link_broken"),
  id: z.string(),
  broken_end: SlotRefSchema,
  surviving_end: SlotRefSchema,
});

export const GraphEventSchema = z.discriminatedUnion("event", [
  NodeCreatedEventSchema,
  NodeRemovedEventSchema,
  NodeRenamedEventSchema,
  SlotChangedEventSchema,
  LifecycleTransitionEventSchema,
  LinkAddedEventSchema,
  LinkRemovedEventSchema,
  LinkBrokenEventSchema,
]);

export type NodeCreatedEvent = z.infer<typeof NodeCreatedEventSchema>;
export type NodeRemovedEvent = z.infer<typeof NodeRemovedEventSchema>;
export type NodeRenamedEvent = z.infer<typeof NodeRenamedEventSchema>;
export type SlotChangedEvent = z.infer<typeof SlotChangedEventSchema>;
export type LifecycleTransitionEvent = z.infer<typeof LifecycleTransitionEventSchema>;
export type LinkAddedEvent = z.infer<typeof LinkAddedEventSchema>;
export type LinkRemovedEvent = z.infer<typeof LinkRemovedEventSchema>;
export type LinkBrokenEvent = z.infer<typeof LinkBrokenEventSchema>;
export type GraphEvent = z.infer<typeof GraphEventSchema>;
