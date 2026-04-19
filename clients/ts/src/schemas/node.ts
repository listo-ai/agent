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
  /**
   * Materialised parent path. `"/"` for depth-1 nodes, `null` for root.
   * Use with `filter=parent_path==<path>` to list direct children only.
   */
  parent_path: z.string().nullable(),
  parent_id: z.string().nullable(),
  lifecycle: LifecycleSchema,
  slots: z.array(SlotSchema),
});

export const PageMetaSchema = z.object({
  total: z.number().int().nonnegative(),
  page: z.number().int().positive(),
  size: z.number().int().positive(),
  pages: z.number().int().nonnegative(),
});

export const NodeListResponseSchema = z.object({
  data: z.array(NodeSnapshotSchema),
  meta: PageMetaSchema,
});

export type Slot = z.infer<typeof SlotSchema>;
export type NodeSnapshot = z.infer<typeof NodeSnapshotSchema>;
export type PageMeta = z.infer<typeof PageMetaSchema>;
export type NodeListResponse = z.infer<typeof NodeListResponseSchema>;
