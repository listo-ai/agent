import type { RequestTransport } from "../transport/request.js";
import {
  BucketedTelemetryResponseSchema,
  GroupedTelemetryResponseSchema,
  HistoryBucketedResponseSchema,
  HistoryResponseSchema,
  TelemetryResponseSchema,
  RecordResultSchema,
} from "../schemas/history.js";
import type {
  BucketedTelemetryResponse,
  GroupedTelemetryResponse,
  HistoryBucketedResponse,
  HistoryRecord,
  RecordResult,
  ScalarRecord,
} from "../schemas/history.js";

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

/** `avg | min | max | sum | last | count` — see QUERY-LANG.md § time-series. */
export type TelemetryAgg = "avg" | "min" | "max" | "sum" | "last" | "count";

/** Structured history only supports `last` or `count`. */
export type HistoryAgg = "last" | "count";

export interface BucketedQueryOptions extends HistoryQueryOptions {
  /** Bucket width in ms (wall-clock aligned). Required. */
  bucket: number;
  /** Aggregation within each bucket. Defaults to `avg`. */
  agg?: TelemetryAgg;
}

export interface HistoryBucketedQueryOptions extends HistoryQueryOptions {
  /** Bucket width in ms. Required. */
  bucket: number;
  /** `last` (default) or `count`. */
  agg?: HistoryAgg;
}

export interface GroupedTelemetryQueryOptions extends HistoryQueryOptions {
  /** Bucket width in ms. Required — raw rows across many nodes are ambiguous. */
  bucket: number;
  agg?: TelemetryAgg;
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
   * Fetch bucketed/aggregated telemetry for a Bool or Number slot.
   * Buckets are wall-clock-aligned by `bucket` ms. See
   * `docs/design/QUERY-LANG.md` § "Time-series query shape".
   */
  listTelemetryBucketed(
    path: string,
    slot: string,
    opts: BucketedQueryOptions,
  ): Promise<BucketedTelemetryResponse>;

  /**
   * Fetch bucketed structured history (String/Json slots) with
   * `agg=last` or `agg=count`. `avg`/`min`/`max`/`sum` are not
   * meaningful for JSON payloads and rejected server-side.
   */
  listHistoryBucketed(
    path: string,
    slot: string,
    opts: HistoryBucketedQueryOptions,
  ): Promise<HistoryBucketedResponse>;

  /**
   * Fan out a bucketed telemetry query across every node of `kind`.
   * Returns one series per matching node. Node-kind is the pragmatic
   * analog to the `group_by=tag` primitive — we don't have tags, but
   * we do have node kinds.
   */
  listTelemetryGrouped(
    kind: string,
    slot: string,
    opts: GroupedTelemetryQueryOptions,
  ): Promise<GroupedTelemetryResponse>;

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

  function appendBucketed(
    qs: URLSearchParams,
    opts: HistoryQueryOptions & { bucket: number; agg?: string },
  ): void {
    if (opts.from !== undefined) qs.set("from", String(opts.from));
    if (opts.to !== undefined) qs.set("to", String(opts.to));
    if (opts.limit !== undefined) qs.set("limit", String(opts.limit));
    qs.set("bucket", String(opts.bucket));
    if (opts.agg) qs.set("agg", opts.agg);
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

    async listTelemetryBucketed(path, slot, opts) {
      const qs = new URLSearchParams({ path, slot });
      appendBucketed(qs, opts);
      const raw = await http.get<unknown>(`${base}/telemetry?${qs.toString()}`);
      return BucketedTelemetryResponseSchema.parse(raw);
    },

    async listHistoryBucketed(path, slot, opts) {
      const qs = new URLSearchParams({ path, slot });
      appendBucketed(qs, opts);
      const raw = await http.get<unknown>(`${base}/history?${qs.toString()}`);
      return HistoryBucketedResponseSchema.parse(raw);
    },

    async listTelemetryGrouped(kind, slot, opts) {
      const qs = new URLSearchParams({ kind, slot });
      appendBucketed(qs, opts);
      const raw = await http.get<unknown>(`${base}/telemetry?${qs.toString()}`);
      return GroupedTelemetryResponseSchema.parse(raw);
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
