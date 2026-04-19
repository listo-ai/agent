import type { RequestTransport } from "../transport/request.js";
import { KindSchema } from "../schemas/kind.js";
import type { Kind } from "../schemas/kind.js";

export interface KindsApi {
  list(): Promise<Kind[]>;
  /** Return only kinds that may be placed under the given node path. */
  listPlaceableUnder(parentPath: string): Promise<Kind[]>;
}

export function createKindsApi(http: RequestTransport, apiVersion: number): KindsApi {
  const base = `/api/v${apiVersion}/kinds`;

  return {
    async list(): Promise<Kind[]> {
      const raw = await http.get<unknown[]>(base);
      return raw.map((entry) => KindSchema.parse(entry));
    },

    async listPlaceableUnder(parentPath: string): Promise<Kind[]> {
      const encoded = encodeURIComponent(parentPath);
      const raw = await http.get<unknown[]>(`${base}?placeable_under=${encoded}`);
      return raw.map((entry) => KindSchema.parse(entry));
    },
  };
}
