import { z } from "zod";

export const SeededNodeSchema = z.object({
  path: z.string(),
  kind: z.string(),
});

export const SeedResultSchema = z.object({
  folder: z.string(),
  nodes: z.array(SeededNodeSchema),
  links: z.array(z.string()),
});

export type SeededNode = z.infer<typeof SeededNodeSchema>;
export type SeedResult = z.infer<typeof SeedResultSchema>;
