import type { HttpClient } from "../transport/http.js";
import { LinkSchema } from "../schemas/link.js";
import type { Link } from "../schemas/link.js";
import { z } from "zod";

/**
 * Links api against:
 *   `GET    /api/v1/links`
 *   `POST   /api/v1/links` `{source:{path|node_id,slot}, target:{...}}` → `{id}`
 *   `DELETE /api/v1/links/:id`
 */

export type LinkEndpointRef =
  | { path: string; slot: string }
  | { node_id: string; slot: string };

const CreateLinkRespSchema = z.object({ id: z.string() });

export interface LinksApi {
  list(): Promise<Link[]>;
  create(source: LinkEndpointRef, target: LinkEndpointRef): Promise<string>;
  remove(id: string): Promise<void>;
}

export function createLinksApi(http: HttpClient, apiVersion: number): LinksApi {
  const base = `/api/v${apiVersion}/links`;
  return {
    async list(): Promise<Link[]> {
      const raw = await http.get<unknown[]>(base);
      return raw.map((r) => LinkSchema.parse(r));
    },
    async create(source, target): Promise<string> {
      const raw = await http.post<unknown>(base, { source, target });
      return CreateLinkRespSchema.parse(raw).id;
    },
    async remove(id): Promise<void> {
      await http.delete(`${base}/${encodeURIComponent(id)}`);
    },
  };
}
