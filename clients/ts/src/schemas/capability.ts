import { z } from "zod";

export const CapabilitySchema = z.object({
  id: z.string(),
  version: z.string(),
});

export const CapabilityManifestSchema = z.object({
  version: z.number().int().positive(),
  capabilities: z.array(CapabilitySchema),
});

export type Capability         = z.infer<typeof CapabilitySchema>;
export type CapabilityManifest = z.infer<typeof CapabilityManifestSchema>;
