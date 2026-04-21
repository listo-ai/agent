import { z } from "zod";
import { FlowRevisionOpSchema } from "./flow.js";

/**
 * One entry in the `node_setting_revisions` append-only log.
 *
 * Mirrors the Rust `NodeSettingRevisionDto` in
 * `crates/transport-rest/src/nodes.rs` (Phase 2 — pending backend wiring).
 *
 * Design reference: docs/design/UNDO-REDO.md § node_setting_revisions
 */
export const NodeSettingRevisionDtoSchema = z.object({
  id: z.string(),
  flow_id: z.string(),
  node_id: z.string(),
  parent_id: z.string().nullable(),
  seq: z.number().int().nonnegative(),
  author: z.string(),
  /**
   * Same vocabulary as flow revisions: `create | edit | undo | redo |
   * revert | import | duplicate | paste`.
   */
  op: FlowRevisionOpSchema,
  /**
   * For `undo` / `redo` / `revert`: the revision whose content was
   * re-materialised. `null` for forward edits.
   */
  target_rev_id: z.string().nullable(),
  /**
   * Kind schema version the payload was authored against.
   * Used for migration on materialisation — see UNDO-REDO.md §
   * "Historical settings materialisation".
   */
  schema_version: z.string(),
  created_at: z.string(),
});

export type NodeSettingRevisionDto = z.infer<typeof NodeSettingRevisionDtoSchema>;

export const NodeSettingRevisionListResponseSchema = z.object({
  data: z.array(NodeSettingRevisionDtoSchema),
  total: z.number().int().nonnegative(),
});

export type NodeSettingRevisionListResponse = z.infer<
  typeof NodeSettingRevisionListResponseSchema
>;

/**
 * The materialised settings payload at a specific revision.
 *
 * When the kind schema has migrated past the revision's `schema_version`,
 * the server may return `migration_status: "unavailable"` or
 * `"kind-missing"` with the raw unmigrated payload.
 */
export const NodeSettingsAtRevisionSchema = z.object({
  revision_id: z.string(),
  schema_version: z.string(),
  current_schema_version: z.string(),
  /** The settings payload — may be raw/unmigrated when migration_status != "ok". */
  payload: z.unknown(),
  migration_status: z.enum(["ok", "unavailable", "kind-missing"]),
});

export type NodeSettingsAtRevision = z.infer<typeof NodeSettingsAtRevisionSchema>;

/**
 * Returned by settings/undo, settings/redo, settings/revert.
 */
export const NodeSettingsMutationResultSchema = z.object({
  head_revision_id: z.string(),
});

export type NodeSettingsMutationResult = z.infer<typeof NodeSettingsMutationResultSchema>;
