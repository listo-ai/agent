// Dashboard UI operations — `GET /api/v1/ui/nav`, `POST /api/v1/ui/resolve`,
// `POST /api/v1/ui/action`, `GET /api/v1/ui/table`.

import type { RequestTransport } from "../transport/request.js";
import {
  UiActionResponseSchema,
  UiNavNodeSchema,
  UiResolveResponseSchema,
  UiTableResponseSchema,
  UiVocabularySchema,
  type UiActionRequest,
  type UiActionResponse,
  type UiNavNode,
  type UiResolveRequest,
  type UiResolveResponse,
  type UiTableParams,
  type UiTableResponse,
  type UiVocabulary,
} from "../schemas/ui.js";

export interface UiApi {
  /** Fetch the ui.nav subtree rooted at `rootId`. */
  nav(rootId: string): Promise<UiNavNode>;

  /**
   * Resolve a ui.page into a render tree. Pass `dry_run: true` on the
   * request to get structured validation errors instead of rendering.
   */
  resolve(req: UiResolveRequest): Promise<UiResolveResponse>;

  /**
   * Dispatch a named action handler and receive a response.
   *
   * Throws on HTTP 404 (unregistered handler) or 422 (handler error).
   */
  action(req: UiActionRequest): Promise<UiActionResponse>;

  /**
   * Fetch a paginated table of nodes matching `params.query`.
   * The `query` string is the RSQL expression from a Table component's
   * `source.query`.
   */
  table(params: UiTableParams): Promise<UiTableResponse>;

  /**
   * Render a node's default SDUI view
   * (`GET /api/v1/ui/render?target=<id>[&view=<id>]`).
   *
   * Looks up the target's kind, picks the highest-priority view (or the
   * view with the given id), substitutes `$target` bindings in the view
   * template, and returns the same response shape as `resolve`.
   */
  render(target: string, view?: string): Promise<UiResolveResponse>;

  /**
   * Fetch the `ui_ir::Component` JSON Schema
   * (`GET /api/v1/ui/vocabulary`). Consumed by Monaco, Studio's palette,
   * and LLM authoring tools.
   */
  vocabulary(): Promise<UiVocabulary>;
}

export function createUiApi(http: RequestTransport, apiVersion: number): UiApi {
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

    async action(req: UiActionRequest): Promise<UiActionResponse> {
      const raw = await http.post<unknown>(`${base}/action`, req);
      return UiActionResponseSchema.parse(raw);
    },

    async table(params: UiTableParams): Promise<UiTableResponse> {
      const qs = new URLSearchParams();
      if (params.query) qs.set("query", params.query);
      if (params.filter) qs.set("filter", params.filter);
      if (params.sort) qs.set("sort", params.sort);
      if (params.page != null) qs.set("page", String(params.page));
      if (params.size != null) qs.set("size", String(params.size));
      if (params.source_id) qs.set("source_id", params.source_id);
      const raw = await http.get<unknown>(
        `${base}/table${qs.size > 0 ? `?${qs.toString()}` : ""}`,
      );
      return UiTableResponseSchema.parse(raw);
    },

    async render(target: string, view?: string): Promise<UiResolveResponse> {
      const qs = new URLSearchParams({ target });
      if (view) qs.set("view", view);
      const raw = await http.get<unknown>(`${base}/render?${qs.toString()}`);
      return UiResolveResponseSchema.parse(raw);
    },

    async vocabulary(): Promise<UiVocabulary> {
      const raw = await http.get<unknown>(`${base}/vocabulary`);
      return UiVocabularySchema.parse(raw);
    },
  };
}
