// Dashboard UI operations — `GET /api/v1/ui/nav`, `POST /api/v1/ui/resolve`.

import type { HttpClient } from "../transport/http.js";
import {
  UiNavNodeSchema,
  UiResolveResponseSchema,
  type UiNavNode,
  type UiResolveRequest,
  type UiResolveResponse,
} from "../schemas/ui.js";

export interface UiApi {
  /** Fetch the ui.nav subtree rooted at `rootId`. */
  nav(rootId: string): Promise<UiNavNode>;

  /**
   * Resolve a ui.page into a render tree. Pass `dry_run: true` on the
   * request to get structured validation errors instead of rendering.
   */
  resolve(req: UiResolveRequest): Promise<UiResolveResponse>;
}

export function createUiApi(http: HttpClient, apiVersion: number): UiApi {
  const base = `/api/v${apiVersion}/ui`;
  return {
    async nav(rootId: string): Promise<UiNavNode> {
      const raw = await http.get<unknown>(
        `${base}/nav?root=${encodeURIComponent(rootId)}`,
      );
      return UiNavNodeSchema.parse(raw);
    },

    async resolve(req: UiResolveRequest): Promise<UiResolveResponse> {
      const raw = await http.post<unknown>(`${base}/resolve`, req);
      return UiResolveResponseSchema.parse(raw);
    },
  };
}
