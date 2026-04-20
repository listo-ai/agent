import { z } from "zod";

/**
 * Tags extracted from the `config.tags` slot of a `sys.auth.user` node.
 * Mirrors `TagsDto` in `crates/transport-rest/src/users.rs`.
 * Field order is load-bearing — do not alphabetize.
 */
export const UserTagsSchema = z
  .object({
    labels: z.array(z.string()).default([]),
    kv: z.record(z.string(), z.string()).default({}),
  })
  .strict();

/**
 * One `sys.auth.user` node as seen by the user-management list view.
 * Mirrors `UserDto` in `crates/transport-rest/src/users.rs`.
 * Field order is load-bearing — do not alphabetize.
 */
export const UserSchema = z
  .object({
    id: z.string(),
    path: z.string(),
    display_name: z.string().nullable(),
    email: z.string().nullable(),
    enabled: z.boolean(),
    tags: UserTagsSchema,
  })
  .strict();

/** Paged response envelope for `GET /api/v1/users`. */
export const UserListResponseSchema = z
  .object({
    data: z.array(UserSchema),
    meta: z.object({
      total: z.number(),
      page: z.number(),
      size: z.number(),
      pages: z.number(),
    }),
  })
  .strict();

/** Request body for `POST /api/v1/users/{id}/grants`. */
export const GrantRoleReqSchema = z
  .object({
    role: z.string(),
    bulk_action_id: z.string(),
  })
  .strict();

/** Response for `POST /api/v1/users/{id}/grants`. */
export const GrantRoleRespSchema = z
  .object({
    user_id: z.string(),
    role: z.string(),
    bulk_action_id: z.string(),
    status: z.string(),
  })
  .strict();

export type UserTags = z.infer<typeof UserTagsSchema>;
export type User = z.infer<typeof UserSchema>;
export type UserListResponse = z.infer<typeof UserListResponseSchema>;
export type GrantRoleReq = z.infer<typeof GrantRoleReqSchema>;
export type GrantRoleResp = z.infer<typeof GrantRoleRespSchema>;
