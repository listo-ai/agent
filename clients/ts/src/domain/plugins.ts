import type { HttpClient } from "../transport/http.js";
import { PluginSummarySchema } from "../schemas/plugin.js";
import type { PluginSummary } from "../schemas/plugin.js";

/**
 * Plugin operations against the Rust REST surface
 * (`crates/transport-rest/src/plugins.rs`):
 *   `GET  /api/v1/plugins`               → PluginSummary[]
 *   `GET  /api/v1/plugins/:id`           → PluginSummary
 *   `POST /api/v1/plugins/:id/enable`    → 204
 *   `POST /api/v1/plugins/:id/disable`   → 204
 *   `POST /api/v1/plugins/reload`        → 204
 *
 * List and detail share `LoadedPluginSummary` on the Rust side so
 * there's no separate `PluginDetail` type here either.
 */
export interface PluginsApi {
  list(): Promise<PluginSummary[]>;
  get(id: string): Promise<PluginSummary>;
  enable(id: string): Promise<void>;
  disable(id: string): Promise<void>;
  /** Rescan the plugins dir. Dev-loop ergonomics. */
  reload(): Promise<void>;
}

export function createPluginsApi(http: HttpClient, apiVersion: number): PluginsApi {
  const base = `/api/v${apiVersion}`;

  return {
    async list(): Promise<PluginSummary[]> {
      const raw = await http.get<unknown[]>(`${base}/plugins`);
      return raw.map((r) => PluginSummarySchema.parse(r));
    },

    async get(id: string): Promise<PluginSummary> {
      const raw = await http.get<unknown>(
        `${base}/plugins/${encodeURIComponent(id)}`,
      );
      return PluginSummarySchema.parse(raw);
    },

    async enable(id: string): Promise<void> {
      await http.postNoContent(
        `${base}/plugins/${encodeURIComponent(id)}/enable`,
        {},
      );
    },

    async disable(id: string): Promise<void> {
      await http.postNoContent(
        `${base}/plugins/${encodeURIComponent(id)}/disable`,
        {},
      );
    },

    async reload(): Promise<void> {
      await http.postNoContent(`${base}/plugins/reload`, {});
    },
  };
}
