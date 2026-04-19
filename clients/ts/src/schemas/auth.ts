import { z } from "zod";

/**
 * Scope atoms — snake_case strings matching Rust's
 * `#[serde(rename_all = "snake_case")]` on `spi::Scope`.
 */
export const ScopeSchema = z.enum([
  "read_nodes",
  "write_nodes",
  "write_slots",
  "write_config",
  "manage_plugins",
  "manage_fleet",
  "admin",
]);

/**
 * Response from `GET /api/v1/auth/whoami`. Mirrors the Rust DTO in
 * `crates/transport-rest/src/auth_routes.rs` and `clients/rs/src/types.rs`.
 * Field order is load-bearing — do not alphabetize.
 */
export const WhoAmISchema = z
  .object({
    actor_kind: z.enum(["user", "machine", "dev_null"]),
    actor_id: z.string().nullable(),
    actor_display: z.string(),
    tenant: z.string(),
    scopes: z.array(ScopeSchema),
    provider: z.string(),
  })
  .strict();

export type Scope = z.infer<typeof ScopeSchema>;
export type WhoAmI = z.infer<typeof WhoAmISchema>;
