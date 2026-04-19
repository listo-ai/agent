import { z } from "zod";

/**
 * Which agent an operation is targeting.
 *
 * Mirrors `spi::fleet::FleetScope` — serde tag `kind`, snake_case.
 * The TypeScript discriminated union round-trips through JSON without
 * any manual mapping.
 *
 * `AgentClient` uses this to pick its transport:
 *   - `local`  → `fetch` against `baseUrl`
 *   - `remote` → fleet req/reply on `fleet.<tenant>.<agent_id>.<kind>.<…>`
 *
 * Broadcast (fan-out) is not in v1 — deferred until `AgentFilter`
 * semantics and partial-reply aggregation have their own design pass.
 */
export const FleetScopeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("local") }),
  z.object({
    kind: z.literal("remote"),
    tenant: z.string(),
    agent_id: z.string(),
  }),
]);

/**
 * Discriminated union type for `FleetScope`.
 * Namespace provides convenience constructors and type guards.
 */
export type FleetScope = z.infer<typeof FleetScopeSchema>;

// eslint-disable-next-line @typescript-eslint/no-namespace
export namespace FleetScope {
  export function local(): FleetScope {
    return { kind: "local" };
  }
  export function remote(tenant: string, agent_id: string): FleetScope {
    return { kind: "remote", tenant, agent_id };
  }
  export function isLocal(s: FleetScope): s is { kind: "local" } {
    return s.kind === "local";
  }
  export function isRemote(
    s: FleetScope,
  ): s is { kind: "remote"; tenant: string; agent_id: string } {
    return s.kind === "remote";
  }
}
