import { z } from "zod";

// ---------------------------------------------------------------------------
// FlowDto  — returned by GET /api/v1/flows and GET /api/v1/flows/{id}
// ---------------------------------------------------------------------------

export const FlowDtoSchema = z.object({
  id: z.string(),
  name: z.string(),
  /** Serialised flow document — the versioned topology blob. */
  document: z.unknown(),
  /**
   * ID of the latest revision. `null` for a freshly-created flow that
   * has never had an edit/undo/redo applied.
   */
  head_revision_id: z.string().nullable(),
  /** Monotonic per-flow sequence counter. 0 before any edits. */
  head_seq: z.number().int().nonnegative(),
});

export type FlowDto = z.infer<typeof FlowDtoSchema>;

// ---------------------------------------------------------------------------
// FlowRevisionDto  — returned by GET /api/v1/flows/{id}/revisions
// ---------------------------------------------------------------------------

/**
 * The `op` field is load-bearing: `"undo"` / `"redo"` entries drive the
 * redo-chain reconstruction and are used by the history UI for scope badges.
 */
export const FlowRevisionOpSchema = z.enum([
  "create",
  "edit",
  "undo",
  "redo",
  "revert",
  "import",
  "duplicate",
  "paste",
]);

export type FlowRevisionOp = z.infer<typeof FlowRevisionOpSchema>;

export const FlowRevisionDtoSchema = z.object({
  id: z.string(),
  flow_id: z.string(),
  parent_id: z.string().nullable(),
  seq: z.number().int().nonnegative(),
  author: z.string(),
  op: FlowRevisionOpSchema,
  /**
   * For `undo` / `redo` / `revert` revisions: the revision whose content
   * was re-materialised. `null` for forward edits.
   */
  target_rev_id: z.string().nullable(),
  summary: z.string(),
  created_at: z.string(),
});

export type FlowRevisionDto = z.infer<typeof FlowRevisionDtoSchema>;

// ---------------------------------------------------------------------------
// FlowMutationResult  — returned by edit / undo / redo / revert
// ---------------------------------------------------------------------------

export const FlowMutationResultSchema = z.object({
  head_revision_id: z.string(),
});

export type FlowMutationResult = z.infer<typeof FlowMutationResultSchema>;

// ---------------------------------------------------------------------------
// FlowListResponse  — paged list of flows
// ---------------------------------------------------------------------------

export const FlowListResponseSchema = z.object({
  data: z.array(FlowDtoSchema),
  total: z.number().int().nonnegative(),
});

export type FlowListResponse = z.infer<typeof FlowListResponseSchema>;

// ---------------------------------------------------------------------------
// FlowRevisionListResponse  — paged list of revisions
// ---------------------------------------------------------------------------

export const FlowRevisionListResponseSchema = z.object({
  data: z.array(FlowRevisionDtoSchema),
  total: z.number().int().nonnegative(),
});

export type FlowRevisionListResponse = z.infer<typeof FlowRevisionListResponseSchema>;
