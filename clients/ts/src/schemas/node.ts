import { z } from "zod";
import { LifecycleSchema } from "./events.js";

/**
 * Node + slot snapshots as returned by `GET /api/v1/nodes` and
 * `GET /api/v1/node?path=...`. Mirrors the Rust `NodeDto` in
 * `crates/transport-rest/src/routes.rs`.
 */

export const SlotSchema = z.object({
  name: z.string(),
  value: z.unknown(),
  generation: z.number().int().nonnegative(),
});

export const NodeSnapshotSchema = z.object({
  id: z.string(),
  kind: z.string(),
  /** Slash-separated materialised path, e.g. `/station/floor3/ahu-1`. */
  path: z.string(),
  parent_id: z.string().nullable(),
  lifecycle: LifecycleSchema,
  slots: z.array(SlotSchema),
});

export type Slot = z.infer<typeof SlotSchema>;
export type NodeSnapshot = z.infer<typeof NodeSnapshotSchema>;
