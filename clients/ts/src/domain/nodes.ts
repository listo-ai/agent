import type { HttpClient } from "../transport/http.js";
import { NodeSnapshotSchema } from "../schemas/node.js";
import type { NodeSnapshot } from "../schemas/node.js";

/**
 * Node operations against the Rust REST surface
 * (`crates/transport-rest/src/routes.rs`):
 *   `GET  /api/v1/nodes`
 *   `GET  /api/v1/node?path=...`
 *   `POST /api/v1/nodes` `{parent, kind, name}` → `{id, path}`
 *
 * Delete is not yet on the wire (Stage 9 territory) so it's omitted
 * rather than stubbed — callers hit a compile-time error instead of a
 * 404 at runtime.
 */
export interface NodesApi {
  getNodes(): Promise<NodeSnapshot[]>;
  getNode(path: string): Promise<NodeSnapshot>;
  createNode(args: {
    parent: string;
    kind: string;
    name: string;
  }): Promise<{ id: string; path: string }>;
}

export function createNodesApi(http: HttpClient, apiVersion: number): NodesApi {
  const base = `/api/v${apiVersion}`;

  return {
    async getNodes(): Promise<NodeSnapshot[]> {
      const raw = await http.get<unknown[]>(`${base}/nodes`);
      return raw.map((r) => NodeSnapshotSchema.parse(r));
    },

    async getNode(path: string): Promise<NodeSnapshot> {
      const raw = await http.get<unknown>(
        `${base}/node?path=${encodeURIComponent(path)}`,
      );
      return NodeSnapshotSchema.parse(raw);
    },

    async createNode(args): Promise<{ id: string; path: string }> {
      return http.post<{ id: string; path: string }>(`${base}/nodes`, args);
    },
  };
}
