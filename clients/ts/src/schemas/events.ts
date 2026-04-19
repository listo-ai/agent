import { z } from "zod";

// Discriminated union of graph events emitted over SSE.

export const SlotChangedEventSchema = z.object({
  type: z.literal("SlotChanged"),
  /** Dot-separated node path, e.g. "tenant.site.device". */
  path: z.string(),
  /** Slot name, e.g. "out" or "status". */
  slot: z.string(),
  /** New slot value. */
  value: z.unknown(),
  /** ISO-8601 timestamp. */
  ts: z.string(),
});

export const NodeAddedEventSchema = z.object({
  type: z.literal("NodeAdded"),
  path: z.string(),
  kind: z.string(),
  ts: z.string(),
});

export const NodeRemovedEventSchema = z.object({
  type: z.literal("NodeRemoved"),
  path: z.string(),
  ts: z.string(),
});

export const GraphEventSchema = z.discriminatedUnion("type", [
  SlotChangedEventSchema,
  NodeAddedEventSchema,
  NodeRemovedEventSchema,
]);

export type SlotChangedEvent = z.infer<typeof SlotChangedEventSchema>;
export type NodeAddedEvent   = z.infer<typeof NodeAddedEventSchema>;
export type NodeRemovedEvent = z.infer<typeof NodeRemovedEventSchema>;
export type GraphEvent       = z.infer<typeof GraphEventSchema>;
