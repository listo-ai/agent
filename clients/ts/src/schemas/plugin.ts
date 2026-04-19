import { z } from "zod";

/**
 * Plugin summary as returned by `GET /api/v1/plugins` and
 * `GET /api/v1/plugins/:id`. Mirrors `LoadedPluginSummary` in
 * `crates/extensions-host/src/registry.rs` — every field here
 * exists on the Rust side; nothing is invented client-side.
 *
 * The list and detail endpoints emit the same shape (no separate
 * `PluginDetail`) — they read from the same struct.
 */

export const PluginLifecycleSchema = z.enum([
  "discovered",
  "validated",
  "enabled",
  "disabled",
  "failed",
]);

export const PluginSummarySchema = z.object({
  id: z.string(),
  version: z.string(),
  lifecycle: PluginLifecycleSchema,
  display_name: z.string().nullable(),
  description: z.string().nullable(),
  has_ui: z.boolean(),
  ui_entry: z.string().nullable(),
  kinds: z.array(z.string()),
  load_errors: z.array(z.string()),
});

export type PluginLifecycle = z.infer<typeof PluginLifecycleSchema>;
export type PluginSummary = z.infer<typeof PluginSummarySchema>;
