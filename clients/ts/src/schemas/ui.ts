// Zod schemas for the dashboard UI surface.
// Field-for-field mirror of `clients/rs/src/types.rs` UI DTOs, which in
// turn mirror the server's `dashboard_transport::{nav,resolve}` types.
// See docs/design/NEW-API.md for the parity rule.

import { z } from "zod";

/** Recursive nav subtree — response for `GET /api/v1/ui/nav`. */
export interface UiNavNode {
  id: string;
  title: string | null;
  path: string | null;
  icon: string | null;
  order: number | null;
  frame_alias: string | null;
  frame_ref?: unknown;
  children: UiNavNode[];
}

export const UiNavNodeSchema: z.ZodType<UiNavNode> = z.lazy(() =>
  z.object({
    id: z.string(),
    title: z.string().nullable(),
    path: z.string().nullable(),
    icon: z.string().nullable(),
    order: z.number().int().nullable(),
    frame_alias: z.string().nullable(),
    // Null or any JSON — server always sends the key, null when absent.
    frame_ref: z.unknown(),
    children: z.array(UiNavNodeSchema),
  }),
);

/** Request body for `POST /api/v1/ui/resolve`. */
export const UiResolveRequestSchema = z.object({
  page_ref: z.string(),
  stack: z.array(z.string()).default([]),
  page_state: z.unknown().default({}),
  dry_run: z.boolean().default(false),
  auth_subject: z.string().nullable().optional(),
  user_claims: z.record(z.string(), z.unknown()).default({}),
});
export type UiResolveRequest = z.infer<typeof UiResolveRequestSchema>;

/** Tagged widget render output. Discriminator `kind`. */
export const UiRenderedWidgetSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("ui.widget"),
    id: z.string(),
    widget_type: z.string(),
    values: z.record(z.string(), z.unknown()),
    layout_hint: z.unknown().optional(),
  }),
  z.object({
    kind: z.literal("ui.widget.forbidden"),
    id: z.string(),
    reason: z.string(),
  }),
  z.object({
    kind: z.literal("ui.widget.dangling"),
    id: z.string(),
  }),
]);
export type UiRenderedWidget = z.infer<typeof UiRenderedWidgetSchema>;

export const UiRenderTreeSchema = z.object({
  page_id: z.string(),
  title: z.string().nullable(),
  widgets: z.array(UiRenderedWidgetSchema),
});
export type UiRenderTree = z.infer<typeof UiRenderTreeSchema>;

export const UiResolveMetaSchema = z.object({
  cache_key: z.number(),
  widget_count: z.number().int().nonnegative(),
  forbidden_count: z.number().int().nonnegative(),
  dangling_count: z.number().int().nonnegative(),
  stack_shadowed: z.array(z.string()),
});
export type UiResolveMeta = z.infer<typeof UiResolveMetaSchema>;

export const UiResolveIssueSchema = z.object({
  location: z.string(),
  message: z.string(),
});
export type UiResolveIssue = z.infer<typeof UiResolveIssueSchema>;

/**
 * Response envelope — untagged. A successful resolve carries
 * `{render, meta}`; a dry run carries `{errors}`. Discriminated by
 * whichever key is present.
 */
export const UiResolveResponseSchema = z.union([
  z.object({
    render: UiRenderTreeSchema,
    meta: UiResolveMetaSchema,
  }),
  z.object({
    errors: z.array(UiResolveIssueSchema),
  }),
]);
export type UiResolveResponse = z.infer<typeof UiResolveResponseSchema>;
