import type { RequestTransport } from "../transport/request.js";
import { NodeListResponseSchema, NodeSnapshotSchema } from "../schemas/node.js";
import type { NodeListResponse, NodeSnapshot } from "../schemas/node.js";

/**
 * Node operations against the Rust REST surface
 * (`crates/transport-rest/src/routes.rs`):
 *   `GET  /api/v1/nodes`
 *   `GET  /api/v1/node?path=...`
 *   `POST /api/v1/nodes` `{parent, kind, name}` → `{id, path}`
 *   `DELETE /api/v1/node?path=...`
 */
export interface NodesApi {
  getNodes(): Promise<NodeSnapshot[]>;
  getNodesPage(params?: {
    filter?: string;
    sort?: string;
    page?: number;
    size?: number;
  }): Promise<NodeListResponse>;
  getNode(path: string): Promise<NodeSnapshot>;
  createNode(args: {
    parent: string;
    kind: string;
    name: string;
  }): Promise<{ id: string; path: string }>;
  removeNode(path: string): Promise<void>;
}

export function createNodesApi(http: RequestTransport, apiVersion: number): NodesApi {
  const base = `/api/v${apiVersion}`;
  const getNodesPage = async (params: {
    filter?: string;
    sort?: string;
    page?: number;
    size?: number;
  } = {}): Promise<NodeListResponse> => {
    const qs = new URLSearchParams();
    if (params.filter) qs.set("filter", params.filter);
    if (params.sort) qs.set("sort", params.sort);
    if (params.page !== undefined) qs.set("page", String(params.page));
    if (params.size !== undefined) qs.set("size", String(params.size));
    const suffix = qs.toString();
    const raw = await http.get<unknown>(
      `${base}/nodes${suffix ? `?${suffix}` : ""}`,
    );
    return NodeListResponseSchema.parse(raw);
  };

  return {
    async getNodes(): Promise<NodeSnapshot[]> {
      const page = await getNodesPage();
      return page.data;
    },

    getNodesPage,

    async getNode(path: string): Promise<NodeSnapshot> {
      const raw = await http.get<unknown>(
        `${base}/node?path=${encodeURIComponent(path)}`,
      );
      return NodeSnapshotSchema.parse(raw);
    },

    async createNode(args): Promise<{ id: string; path: string }> {
      return http.post<{ id: string; path: string }>(`${base}/nodes`, args);
    },

    async removeNode(path: string): Promise<void> {
      await http.delete(`${base}/node?path=${encodeURIComponent(path)}`);
    },
  };
}
