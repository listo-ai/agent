// Zod schemas for the dashboard UI surface.
// Field-for-field mirror of `clients/rs/src/types.rs` UI DTOs, which in
// turn mirror the server's `dashboard_transport::{nav,resolve}` and
// `ui_ir` types. See docs/design/NEW-API.md for the parity rule.

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

// ---- Action ---------------------------------------------------------------

export const UiActionSchema = z.object({
  handler: z.string(),
  args: z.unknown().optional(),
});
export type UiAction = z.infer<typeof UiActionSchema>;

// ---- Table helpers --------------------------------------------------------

export const UiTableSourceSchema = z.object({
  query: z.string(),
  subscribe: z.boolean().optional(),
});
export type UiTableSource = z.infer<typeof UiTableSourceSchema>;

export const UiTableColumnSchema = z.object({
  title: z.string(),
  field: z.string(),
  sortable: z.boolean().optional(),
});
export type UiTableColumn = z.infer<typeof UiTableColumnSchema>;

export const UiDiffAnnotationSchema = z.object({
  line: z.number().int(),
  text: z.string(),
  author: z.string().optional(),
  created_at: z.string().optional(),
});
export type UiDiffAnnotation = z.infer<typeof UiDiffAnnotationSchema>;

// ---- Component (recursive) ------------------------------------------------

/** Recursive component tree node. Discriminator `type`. */
export interface UiComponent {
  type: string;
  [key: string]: unknown;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const UiTabSchema: z.ZodType<any> = z.lazy(() =>
  z.object({
    id: z.string().optional(),
    label: z.string(),
    children: z.array(UiComponentSchema),
  }),
);
export type UiTab = { id?: string; label: string; children: UiComponent[] };

export const UiComponentSchema: z.ZodType<UiComponent> = z.lazy(() =>
  z.discriminatedUnion("type", [
    // layout
    z.object({
      type: z.literal("page"),
      id: z.string(),
      title: z.string().nullable().optional(),
      children: z.array(UiComponentSchema).default([]),
    }),
    z.object({
      type: z.literal("row"),
      id: z.string().optional(),
      children: z.array(UiComponentSchema).default([]),
      gap: z.string().optional(),
    }),
    z.object({
      type: z.literal("col"),
      id: z.string().optional(),
      children: z.array(UiComponentSchema).default([]),
      gap: z.string().optional(),
    }),
    z.object({
      type: z.literal("grid"),
      id: z.string().optional(),
      children: z.array(UiComponentSchema).default([]),
      columns: z.string().optional(),
    }),
    z.object({
      type: z.literal("tabs"),
      id: z.string().optional(),
      tabs: z.array(UiTabSchema),
    }),
    // display
    z.object({
      type: z.literal("text"),
      id: z.string().optional(),
      content: z.string(),
      intent: z.string().optional(),
    }),
    z.object({
      type: z.literal("heading"),
      id: z.string().optional(),
      content: z.string(),
      level: z.number().int().optional(),
    }),
    z.object({
      type: z.literal("badge"),
      id: z.string().optional(),
      label: z.string(),
      intent: z.string().optional(),
    }),
    z.object({
      type: z.literal("diff"),
      id: z.string().optional(),
      old_text: z.string(),
      new_text: z.string(),
      language: z.string().optional(),
      annotations: z.array(UiDiffAnnotationSchema).default([]),
      line_action: UiActionSchema.optional(),
    }),
    // data
    z.object({
      type: z.literal("table"),
      id: z.string().optional(),
      source: UiTableSourceSchema,
      columns: z.array(UiTableColumnSchema),
      row_action: UiActionSchema.optional(),
      page_size: z.number().int().optional(),
    }),
    // input
    z.object({
      type: z.literal("rich_text"),
      id: z.string().optional(),
      value: z.string().optional(),
      placeholder: z.string().optional(),
    }),
    // interactive
    z.object({
      type: z.literal("button"),
      id: z.string().optional(),
      label: z.string(),
      intent: z.string().optional(),
      disabled: z.boolean().optional(),
      action: UiActionSchema.optional(),
    }),
    // composite
    z.object({
      type: z.literal("form"),
      id: z.string().optional(),
      schema_ref: z.string(),
      bindings: z.unknown().optional(),
      submit: UiActionSchema.optional(),
    }),
    // placeholder stubs
    z.object({
      type: z.literal("forbidden"),
      id: z.string(),
      reason: z.string(),
    }),
    z.object({
      type: z.literal("dangling"),
      id: z.string(),
    }),
    // escape hatch — custom renderer
    z.object({
      type: z.literal("custom"),
      id: z.string().optional(),
      renderer_id: z.string(),
      props: z.unknown().optional(),
      subscribe: z.array(z.string()).default([]),
    }),
  ]),
);

/** Root of a resolved component tree. */
export const UiComponentTreeSchema = z.object({
  ir_version: z.number().int().nonnegative(),
  root: UiComponentSchema,
});
export type UiComponentTree = z.infer<typeof UiComponentTreeSchema>;

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

/** Per-widget subscription plan. Subjects are `node.<id>.slot.<name>`. */
export const UiSubscriptionPlanSchema = z.object({
  widget_id: z.string(),
  subjects: z.array(z.string()),
  debounce_ms: z.number().int().nonnegative(),
});
export type UiSubscriptionPlan = z.infer<typeof UiSubscriptionPlanSchema>;

/**
 * Response envelope — untagged. A successful resolve carries
 * `{render, subscriptions, meta}`; a dry run carries `{errors}`.
 * Discriminated by whichever key is present.
 */
export const UiResolveResponseSchema = z.union([
  z.object({
    render: UiComponentTreeSchema,
    subscriptions: z.array(UiSubscriptionPlanSchema),
    meta: UiResolveMetaSchema,
  }),
  z.object({
    errors: z.array(UiResolveIssueSchema),
  }),
]);
export type UiResolveResponse = z.infer<typeof UiResolveResponseSchema>;

// ---- action ----------------------------------------------------------------

export const UiActionContextSchema = z.object({
  target: z.string().optional(),
  stack: z.array(z.string()).default([]),
  page_state: z.record(z.unknown()).default({}),
  auth_subject: z.string().optional(),
});
export type UiActionContext = z.infer<typeof UiActionContextSchema>;

export const UiActionRequestSchema = z.object({
  handler: z.string(),
  args: z.unknown().default(null),
  context: UiActionContextSchema.default({}),
});
export type UiActionRequest = z.infer<typeof UiActionRequestSchema>;

export const UiNavigateToSchema = z.object({
  target_ref: z.string(),
});
export type UiNavigateTo = z.infer<typeof UiNavigateToSchema>;

/**
 * Tagged-union response from `POST /api/v1/ui/action`.
 * Discriminated by the `type` field.
 */
export const UiActionResponseSchema = z.discriminatedUnion("type", [
  z.object({
    type: z.literal("patch"),
    target_component_id: z.string(),
    tree: UiComponentTreeSchema,
  }),
  z.object({
    type: z.literal("navigate"),
    to: UiNavigateToSchema,
  }),
  z.object({
    type: z.literal("full_render"),
    tree: UiComponentTreeSchema,
  }),
  z.object({
    type: z.literal("toast"),
    intent: z.enum(["ok", "warn", "danger"]),
    message: z.string(),
  }),
  z.object({
    type: z.literal("form_errors"),
    errors: z.record(z.string()),
  }),
  z.object({
    type: z.literal("download"),
    url: z.string().url(),
  }),
  z.object({
    type: z.literal("stream"),
    channel: z.string(),
  }),
  z.object({
    type: z.literal("none"),
  }),
]);
export type UiActionResponse = z.infer<typeof UiActionResponseSchema>;

// ---- table -----------------------------------------------------------------

/** Query params for `GET /api/v1/ui/table`. */
export const UiTableParamsSchema = z.object({
  /** Base RSQL query (from the Table component's source.query). */
  query: z.string().default(""),
  /** Additional RSQL clauses merged server-side. */
  filter: z.string().optional(),
  sort: z.string().optional(),
  page: z.number().int().positive().optional(),
  size: z.number().int().positive().optional(),
  /** Table component id for audit. */
  source_id: z.string().optional(),
});
export type UiTableParams = z.infer<typeof UiTableParamsSchema>;

/** A single node row returned by `GET /api/v1/ui/table`. */
export const UiTableRowSchema = z.object({
  id: z.string(),
  kind: z.string(),
  path: z.string(),
  parent_id: z.string().nullable().optional(),
  slots: z.record(z.unknown()),
});
export type UiTableRow = z.infer<typeof UiTableRowSchema>;

export const UiTableMetaSchema = z.object({
  total: z.number().int().nonnegative(),
  page: z.number().int().positive(),
  size: z.number().int().positive(),
  pages: z.number().int().nonnegative(),
});
export type UiTableMeta = z.infer<typeof UiTableMetaSchema>;

/** Response from `GET /api/v1/ui/table`. */
export const UiTableResponseSchema = z.object({
  data: z.array(UiTableRowSchema),
  meta: UiTableMetaSchema,
});
export type UiTableResponse = z.infer<typeof UiTableResponseSchema>;
