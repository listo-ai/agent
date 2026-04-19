import type { HttpClient } from "../transport/http.js";
import { WhoAmISchema } from "../schemas/auth.js";
import type { WhoAmI } from "../schemas/auth.js";

export interface AuthApi {
  whoami(): Promise<WhoAmI>;
}

export function createAuthApi(http: HttpClient, apiVersion: number): AuthApi {
  const base = `/api/v${apiVersion}/auth`;

  return {
    async whoami(): Promise<WhoAmI> {
      const raw = await http.get<unknown>(`${base}/whoami`);
      return WhoAmISchema.parse(raw);
    },
  };
}
