import { z } from "zod";

/**
 * Wire shape for `GET /api/v1/links` and `POST /api/v1/links`.
 * Mirrors `LinkDto` / `EndpointDto` in
 * `crates/transport-rest/src/routes.rs`.
 */

export const LinkEndpointSchema = z.object({
  node_id: z.string(),
  path: z.string().nullable().optional(),
  slot: z.string(),
});

export const LinkSchema = z.object({
  id: z.string(),
  source: LinkEndpointSchema,
  target: LinkEndpointSchema,
});

export type LinkEndpoint = z.infer<typeof LinkEndpointSchema>;
export type Link = z.infer<typeof LinkSchema>;
