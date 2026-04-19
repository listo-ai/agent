import type { HttpClient } from "../transport/http.js";
import { z } from "zod";

/**
 * `POST /api/v1/seed` `{preset}` — instantiates a pre-wired flow shape
 * in the graph. Not a public API — these presets exist for manual
 * testing and the count-chain example; production graphs come from
 * flow documents (Stage 2b) or extension installs (Stage 10).
 */

export type SeedPreset = "count_chain" | "trigger_demo";

const SeededNodeSchema = z.object({
  path: z.string(),
  kind: z.string(),
});

const SeedResultSchema = z.object({
  folder: z.string(),
  nodes: z.array(SeededNodeSchema),
  links: z.array(z.string()),
});

export type SeedResult = z.infer<typeof SeedResultSchema>;

export interface SeedApi {
  apply(preset: SeedPreset): Promise<SeedResult>;
}

export function createSeedApi(http: HttpClient, apiVersion: number): SeedApi {
  const base = `/api/v${apiVersion}/seed`;
  return {
    async apply(preset: SeedPreset): Promise<SeedResult> {
      const raw = await http.post<unknown>(base, { preset });
      return SeedResultSchema.parse(raw);
    },
  };
}
