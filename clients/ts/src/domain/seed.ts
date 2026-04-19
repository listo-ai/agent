import type { HttpClient } from "../transport/http.js";
import { SeedResultSchema } from "../schemas/seed.js";

/**
 * `POST /api/v1/seed` `{preset}` — instantiates a pre-wired flow shape
 * in the graph. Not a public API — these presets exist for manual
 * testing and the count-chain example; production graphs come from
 * flow documents (Stage 2b) or extension installs (Stage 10).
 */

export type SeedPreset = "count_chain" | "trigger_demo";

export type SeedResult = ReturnType<typeof SeedResultSchema.parse>;

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
