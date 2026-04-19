import type { RequestTransport } from "../transport/request.js";
import { z } from "zod";

/**
 * `POST /api/v1/config` `{path, config}` — replaces the node's config
 * blob and re-runs `on_init`. Idempotent for well-behaved behaviours.
 *
 * No GET endpoint is shipped: the config isn't mirrored into the node
 * snapshot today. When `crates/transport-rest` adds one, extend this
 * module — don't stub it.
 */

const NodeConfigSchema = z.record(z.string(), z.unknown());
export type NodeConfig = z.infer<typeof NodeConfigSchema>;

export interface ConfigApi {
  /** Replace a node's config and re-fire `on_init`. */
  setConfig(path: string, config: NodeConfig): Promise<void>;
}

export function createConfigApi(http: RequestTransport, apiVersion: number): ConfigApi {
  const base = `/api/v${apiVersion}`;
  return {
    async setConfig(path: string, config: NodeConfig): Promise<void> {
      await http.postNoContent(`${base}/config`, { path, config });
    },
  };
}
