import type { HttpClient } from "../transport/http.js";
import { z } from "zod";

// Node configuration is an open object — the shape is kind-specific.
// We validate it's a record; deeper validation is the caller's concern.
const ConfigSchema = z.record(z.string(), z.unknown());
export type NodeConfig = z.infer<typeof ConfigSchema>;

export interface ConfigApi {
  getConfig(nodePath: string): Promise<NodeConfig>;
  setConfig(nodePath: string, config: NodeConfig): Promise<void>;
}

export function createConfigApi(http: HttpClient, apiVersion: number): ConfigApi {
  const base = `/api/v${apiVersion}/nodes`;

  return {
    async getConfig(nodePath: string): Promise<NodeConfig> {
      const raw = await http.get<unknown>(`${base}/${encodeURIComponent(nodePath)}/config`);
      return ConfigSchema.parse(raw);
    },

    async setConfig(nodePath: string, config: NodeConfig): Promise<void> {
      await http.put<void>(`${base}/${encodeURIComponent(nodePath)}/config`, config);
    },
  };
}
