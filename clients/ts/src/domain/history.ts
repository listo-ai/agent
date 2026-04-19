import type { RequestTransport } from "../transport/request.js";
import {
  HistoryResponseSchema,
  TelemetryResponseSchema,
  RecordResultSchema,
} from "../schemas/history.js";
import type { HistoryRecord, ScalarRecord, RecordResult } from "../schemas/history.js";

/**
 * History and telemetry endpoints.
 *
 * REST surface (mirrors `crates/transport-rest/src/history.rs`):
 *   `GET  /api/v1/history`         — String / Json / Binary slot history
 *   `GET  /api/v1/telemetry`       — Bool / Number scalar history
 *   `POST /api/v1/history/record`  — on-demand snapshot of a live slot value
 *
 * Storage routing is slot-kind-based (handled server-side):
 *   String / Json / Binary → `history_repo`  (`slot_history` table)
 *   Bool   / Number        → `telemetry_repo` (`slot_timeseries` table)
 */

export interface HistoryQueryOptions {
  /** Inclusive start timestamp, Unix ms. Defaults to 0. */
  from?: number;
  /** Inclusive end timestamp, Unix ms. Defaults to server-side "now". */
  to?: number;
  /** Maximum rows to return. Defaults to 1000. */
  limit?: number;
}

export interface HistoryApi {
  /**
   * Fetch recorded history for a String, Json, or Binary slot.
   * Returns records ordered by `ts_ms` ascending.
   */
  listHistory(
    path: string,
    slot: string,
    opts?: HistoryQueryOptions,
  ): Promise<HistoryRecord[]>;

  /**
   * Fetch recorded scalar series for a Bool or Number slot.
   * Returns records ordered by `ts_ms` ascending.
   */
  listTelemetry(
    path: string,
    slot: string,
    opts?: HistoryQueryOptions,
  ): Promise<ScalarRecord[]>;

  /**
   * Read the slot's current live value from the graph and persist it to the
   * appropriate store immediately. Useful for on-demand snapshots.
   *
   * Returns `{ recorded: true, kind }` on success.
   * Throws if the slot value is `null` (422) or no store is configured (503).
   */
  record(path: string, slot: string): Promise<RecordResult>;
}

export function createHistoryApi(
  http: RequestTransport,
  apiVersion: number,
): HistoryApi {
  const base = `/api/v${apiVersion}`;

  function buildQs(
    path: string,
    slot: string,
    opts: HistoryQueryOptions = {},
  ): string {
    const qs = new URLSearchParams({ path, slot });
    if (opts.from !== undefined) qs.set("from", String(opts.from));
    if (opts.to !== undefined) qs.set("to", String(opts.to));
    if (opts.limit !== undefined) qs.set("limit", String(opts.limit));
    return qs.toString();
  }

  return {
    async listHistory(path, slot, opts) {
      const raw = await http.get<unknown>(
        `${base}/history?${buildQs(path, slot, opts)}`,
      );
      return HistoryResponseSchema.parse(raw).data;
    },

    async listTelemetry(path, slot, opts) {
      const raw = await http.get<unknown>(
        `${base}/telemetry?${buildQs(path, slot, opts)}`,
      );
      return TelemetryResponseSchema.parse(raw).data;
    },

    async record(path, slot) {
      const raw = await http.post<unknown>(`${base}/history/record`, {
        path,
        slot,
      });
      return RecordResultSchema.parse(raw);
    },
  };
}
