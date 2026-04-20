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

/**
 * Process-plugin runtime state — mirrors
 * `PluginRuntimeState` in `crates/extensions-host/src/host.rs`.
 * Discriminated on `status`; some variants carry extra fields.
 */
export const PluginRuntimeStateSchema = z.discriminatedUnion("status", [
  z.object({ status: z.literal("idle") }),
  z.object({ status: z.literal("starting") }),
  z.object({ status: z.literal("ready") }),
  z.object({ status: z.literal("degraded"), detail: z.string() }),
  z.object({
    status: z.literal("restarting"),
    attempt: z.number().int().nonnegative(),
    backoff_ms: z.number().int().nonnegative(),
    reason: z.string(),
  }),
  z.object({ status: z.literal("failed"), reason: z.string() }),
  z.object({ status: z.literal("stopped") }),
]);

/**
 * One entry in `GET /api/v1/plugins/runtime`. The server flattens
 * `PluginRuntimeState` into the object so `id` sits alongside the
 * status fields.
 */
export const PluginRuntimeEntrySchema = z.intersection(
  z.object({ id: z.string() }),
  PluginRuntimeStateSchema,
);

export type PluginRuntimeState = z.infer<typeof PluginRuntimeStateSchema>;
export type PluginRuntimeEntry = z.infer<typeof PluginRuntimeEntrySchema>;
