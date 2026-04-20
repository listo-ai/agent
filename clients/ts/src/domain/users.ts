import type { RequestTransport } from "../transport/request.js";
import {
  UserListResponseSchema,
  GrantRoleRespSchema,
} from "../schemas/user.js";
import type { User, GrantRoleReq, GrantRoleResp } from "../schemas/user.js";

export interface UsersApi {
  /**
   * List `sys.auth.user` nodes with optional tag-aware filtering.
   *
   * Supports `filter=tags.labels=contains=ops` and
   * `filter=tags.kv.site==abc` via the user-management query schema.
   */
  list(params?: {
    filter?: string;
    sort?: string;
    page?: number;
    size?: number;
  }): Promise<User[]>;

  /**
   * Submit a per-user role-grant request.
   *
   * Returns 202 Accepted. Zitadel fan-out is a future landing;
   * `bulk_action_id` threads the session for audit correlation.
   */
  grantRole(userId: string, req: GrantRoleReq): Promise<GrantRoleResp>;
}

export function createUsersApi(http: RequestTransport, apiVersion: number): UsersApi {
  const base = `/api/v${apiVersion}/users`;

  return {
    async list(params = {}): Promise<User[]> {
      const qs = new URLSearchParams();
      if (params.filter !== undefined) qs.set("filter", params.filter);
      if (params.sort !== undefined) qs.set("sort", params.sort);
      if (params.page !== undefined) qs.set("page", String(params.page));
      if (params.size !== undefined) qs.set("size", String(params.size));
      const url = qs.size > 0 ? `${base}?${qs}` : base;
      const raw = await http.get<unknown>(url);
      return UserListResponseSchema.parse(raw).data;
    },

    async grantRole(userId: string, req: GrantRoleReq): Promise<GrantRoleResp> {
      const url = `${base}/${encodeURIComponent(userId)}/grants`;
      const raw = await http.post<unknown>(url, req);
      return GrantRoleRespSchema.parse(raw);
    },
  };
}
