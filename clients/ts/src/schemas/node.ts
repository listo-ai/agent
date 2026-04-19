import { z } from "zod";

export const SlotSchema = z.object({
  name: z.string(),
  value: z.unknown(),
  /** ISO-8601 last-updated timestamp. */
  updatedAt: z.string().optional(),
});

export const NodeSnapshotSchema = z.object({
  /** Dot-separated fully-qualified path. */
  path: z.string(),
  /** Kind identifier, e.g. "builtin.count". */
  kind: z.string(),
  slots: z.record(z.string(), SlotSchema),
  /** Child node paths (shallow). */
  children: z.array(z.string()).optional(),
});

export type Slot         = z.infer<typeof SlotSchema>;
export type NodeSnapshot = z.infer<typeof NodeSnapshotSchema>;
