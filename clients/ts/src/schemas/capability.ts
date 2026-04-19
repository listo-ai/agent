import { z } from "zod";

/**
 * Host-provided capability manifest.
 *
 * Wire shape mirrors the Rust `transport_rest::capabilities::CapabilityManifest`
 * type — see `crates/transport-rest/src/capabilities.rs`.
 */

export const CapabilitySchema = z.object({
  id: z.string(),
  version: z.string(),
  deprecated_since: z.string().optional(),
  removal_planned: z.string().optional(),
});

export const PlatformInfoSchema = z.object({
  version: z.string(),
  flow_schema: z.number().int().nonnegative(),
  node_schema: z.number().int().nonnegative(),
});

export const ApiInfoSchema = z.object({
  rest: z.string(),
});

export const CapabilityManifestSchema = z.object({
  platform: PlatformInfoSchema,
  api: ApiInfoSchema,
  capabilities: z.array(CapabilitySchema),
});

export type Capability = z.infer<typeof CapabilitySchema>;
export type PlatformInfo = z.infer<typeof PlatformInfoSchema>;
export type ApiInfo = z.infer<typeof ApiInfoSchema>;
export type CapabilityManifest = z.infer<typeof CapabilityManifestSchema>;
