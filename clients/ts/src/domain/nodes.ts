import type { HttpClient } from "../transport/http.js";
import { NodeSnapshotSchema } from "../schemas/node.js";
import type { NodeSnapshot } from "../schemas/node.js";

// Pure typed operations for nodes — no fetch calls here, only HttpClient.

export interface NodesApi {
  getNodes(): Promise<NodeSnapshot[]>;
  getNode(path: string): Promise<NodeSnapshot>;
  createNode(path: string, kind: string): Promise<NodeSnapshot>;
  deleteNode(path: string): Promise<void>;
}

export function createNodesApi(http: HttpClient, apiVersion: number): NodesApi {
  const base = `/api/v${apiVersion}/nodes`;

  return {
    async getNodes(): Promise<NodeSnapshot[]> {
      const raw = await http.get<unknown[]>(base);
      return raw.map((r) => NodeSnapshotSchema.parse(r));
    },

    async getNode(path: string): Promise<NodeSnapshot> {
      const raw = await http.get<unknown>(`${base}/${encodeURIComponent(path)}`);
      return NodeSnapshotSchema.parse(raw);
    },

    async createNode(path: string, kind: string): Promise<NodeSnapshot> {
      const raw = await http.post<unknown>(base, { path, kind });
      return NodeSnapshotSchema.parse(raw);
    },

    async deleteNode(path: string): Promise<void> {
      await http.post<void>(`${base}/${encodeURIComponent(path)}/delete`, {});
    },
  };
}
