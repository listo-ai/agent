import { z } from "zod";

export const FacetSchema = z.enum([
  "isProtocol",
  "isDriver",
  "isDevice",
  "isPoint",
  "isCompute",
  "isSystem",
  "isContainer",
  "isWritable",
  "isFlow",
]);

export const ParentMatcherSchema = z.union([
  z.object({ kind: z.string() }),
  z.object({ facet: FacetSchema }),
]);

export const ContainmentSchema = z.object({
  must_live_under: z.array(ParentMatcherSchema),
  may_contain: z.array(ParentMatcherSchema),
  cardinality_per_parent: z.string(),
  cascade: z.string(),
});

export const SlotRoleSchema = z.enum(["config", "input", "output", "status"]);

export const SlotDefinitionSchema = z.object({
  name: z.string(),
  role: SlotRoleSchema,
  value_schema: z.unknown(),
  writable: z.boolean(),
  trigger: z.boolean(),
});

export const KindSchema = z.object({
  id: z.string(),
  display_name: z.string().nullable().optional(),
  facets: z.array(FacetSchema).default([]),
  containment: ContainmentSchema,
  slots: z.array(SlotDefinitionSchema).default([]),
  settings_schema: z.unknown(),
  msg_overrides: z.record(z.string(), z.string()).default({}),
  trigger_policy: z.string().default("on_any"),
  schema_version: z.number().int().nonnegative().default(1),
  placement_class: z.string(),
});

export type Facet = z.infer<typeof FacetSchema>;
export type ParentMatcher = z.infer<typeof ParentMatcherSchema>;
export type Containment = z.infer<typeof ContainmentSchema>;
export type SlotRole = z.infer<typeof SlotRoleSchema>;
export type SlotDefinition = z.infer<typeof SlotDefinitionSchema>;
export type Kind = z.infer<typeof KindSchema>;
