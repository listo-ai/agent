import type { RequestTransport } from "../transport/request.js";
import {
  PluginRuntimeEntrySchema,
  PluginRuntimeStateSchema,
  PluginSummarySchema,
} from "../schemas/block.js";
import type {
  PluginRuntimeEntry,
  PluginRuntimeState,
  PluginSummary,
} from "../schemas/block.js";

/**
 * Block operations against the Rust REST surface
 * (`crates/transport-rest/src/blocks.rs`):
 *   `GET  /api/v1/blocks`               → PluginSummary[]
 *   `GET  /api/v1/blocks/:id`           → PluginSummary
 *   `POST /api/v1/blocks/:id/enable`    → 204
 *   `POST /api/v1/blocks/:id/disable`   → 204
 *   `POST /api/v1/blocks/reload`        → 204
 *
 * List and detail share `LoadedPluginSummary` on the Rust side so
 * there's no separate `PluginDetail` type here either.
 */
export interface PluginsApi {
  list(): Promise<PluginSummary[]>;
  get(id: string): Promise<PluginSummary>;
  enable(id: string): Promise<void>;
  disable(id: string): Promise<void>;
  /** Rescan the blocks dir. Dev-loop ergonomics. */
  reload(): Promise<void>;
  /** Runtime state for a single process block. */
  runtime(id: string): Promise<PluginRuntimeState>;
  /** Snapshot of all process-block runtime states. */
  runtimeAll(): Promise<PluginRuntimeEntry[]>;
}

export function createPluginsApi(http: RequestTransport, apiVersion: number): PluginsApi {
  const base = `/api/v${apiVersion}`;

  return {
    async list(): Promise<PluginSummary[]> {
      const raw = await http.get<unknown[]>(`${base}/blocks`);
      return raw.map((r) => PluginSummarySchema.parse(r));
    },

    async get(id: string): Promise<PluginSummary> {
      const raw = await http.get<unknown>(
        `${base}/blocks/${encodeURIComponent(id)}`,
      );
      return PluginSummarySchema.parse(raw);
    },

    async enable(id: string): Promise<void> {
      await http.postNoContent(
        `${base}/blocks/${encodeURIComponent(id)}/enable`,
        {},
      );
    },

    async disable(id: string): Promise<void> {
      await http.postNoContent(
        `${base}/blocks/${encodeURIComponent(id)}/disable`,
        {},
      );
    },

    async reload(): Promise<void> {
      await http.postNoContent(`${base}/blocks/reload`, {});
    },

    async runtime(id: string): Promise<PluginRuntimeState> {
      const raw = await http.get<unknown>(
        `${base}/blocks/${encodeURIComponent(id)}/runtime`,
      );
      return PluginRuntimeStateSchema.parse(raw);
    },

    async runtimeAll(): Promise<PluginRuntimeEntry[]> {
      const raw = await http.get<unknown[]>(`${base}/blocks/runtime`);
      return raw.map((r) => PluginRuntimeEntrySchema.parse(r));
    },
  };
}
