import type { HttpClient } from "../transport/http.js";
import { KindSchema } from "../schemas/kind.js";
import type { Kind } from "../schemas/kind.js";

export interface KindsApi {
  list(): Promise<Kind[]>;
}

export function createKindsApi(http: HttpClient, apiVersion: number): KindsApi {
  const base = `/api/v${apiVersion}/kinds`;

  return {
    async list(): Promise<Kind[]> {
      const raw = await http.get<unknown[]>(base);
      return raw.map((entry) => KindSchema.parse(entry));
    },
  };
}
