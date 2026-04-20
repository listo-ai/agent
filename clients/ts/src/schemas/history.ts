import { z } from "zod";

/**
 * Wire shapes for the history/telemetry endpoints.
 * Mirrors the Rust DTOs in `crates/transport-rest/src/history.rs`:
 *   `HistoryRecordDto`, `ScalarRecordDto`, and the record-result body.
 */

// ---- history (String / Json / Binary slots) --------------------------------

export const HistoryRecordSchema = z.object({
  id: z.number().int(),
  node_id: z.string(),
  slot_name: z.string(),
  /** "string" | "json" | "binary" */
  slot_kind: z.string(),
  ts_ms: z.number().int(),
  /** Decoded value for String/Json records; `null` for Binary. */
  value: z.unknown().nullable(),
  byte_size: z.number().int(),
  ntp_synced: z.boolean(),
  last_sync_age_ms: z.number().int().nullable().optional(),
});

export const HistoryResponseSchema = z.object({
  data: z.array(HistoryRecordSchema),
});

// ---- telemetry (Bool / Number slots) ---------------------------------------

export const ScalarRecordSchema = z.object({
  node_id: z.string(),
  slot_name: z.string(),
  ts_ms: z.number().int(),
  /** `true`/`false` for Bool slots; numeric for Number slots. */
  value: z.union([z.boolean(), z.number(), z.null()]),
  ntp_synced: z.boolean(),
  last_sync_age_ms: z.number().int().nullable().optional(),
});

export const TelemetryResponseSchema = z.object({
  data: z.array(ScalarRecordSchema),
});

// ---- bucketed telemetry (GET /telemetry?bucket=…&agg=…) -------------------

export const BucketedRowSchema = z.object({
  ts_ms: z.number().int(),
  /** `null` when no numeric samples fell in the bucket. */
  value: z.number().nullable(),
  count: z.number().int().nonnegative(),
});

export const BucketedMetaSchema = z.object({
  bucket_ms: z.number().int().positive(),
  agg: z.string(),
  from: z.number().int(),
  to: z.number().int(),
  bucket_count: z.number().int().nonnegative(),
  /**
   * The first bucket starts before `from` — its aggregate includes
   * only samples within `[from, to]`, but the bucket itself straddles
   * the left edge of the requested window.
   */
  edge_partial_start: z.boolean().default(false),
  /** The last bucket's end extends past `to`. */
  edge_partial_end: z.boolean().default(false),
});

export const BucketedTelemetryResponseSchema = z.object({
  data: z.array(BucketedRowSchema),
  meta: BucketedMetaSchema,
});

// ---- bucketed history (GET /history?bucket=…&agg=last|count) --------------

export const HistoryBucketedRowSchema = z.object({
  ts_ms: z.number().int(),
  /** For `agg=last`: decoded JSON/string payload. `null` for count/binary. */
  value: z.unknown().nullable(),
  /** `"string" | "json" | "binary"`; `null` when bucket is empty. */
  slot_kind: z.string().nullable().optional(),
  count: z.number().int().nonnegative(),
});

export const HistoryBucketedMetaSchema = z.object({
  bucket_ms: z.number().int().positive(),
  /** `"last"` or `"count"` for structured history. */
  agg: z.string(),
  from: z.number().int(),
  to: z.number().int(),
  bucket_count: z.number().int().nonnegative(),
  edge_partial_start: z.boolean().default(false),
  edge_partial_end: z.boolean().default(false),
});

export const HistoryBucketedResponseSchema = z.object({
  data: z.array(HistoryBucketedRowSchema),
  meta: HistoryBucketedMetaSchema,
});

// ---- grouped telemetry (GET /telemetry?kind=…) ----------------------------

export const GroupedSeriesSchema = z.object({
  node_id: z.string(),
  path: z.string(),
  data: z.array(BucketedRowSchema),
  bucket_count: z.number().int().nonnegative(),
});

export const GroupedTelemetryMetaSchema = z.object({
  kind: z.string(),
  slot: z.string(),
  bucket_ms: z.number().int().positive(),
  agg: z.string(),
  from: z.number().int(),
  to: z.number().int(),
  node_count: z.number().int().nonnegative(),
});

export const GroupedTelemetryResponseSchema = z.object({
  series: z.array(GroupedSeriesSchema),
  meta: GroupedTelemetryMetaSchema,
});

// ---- record-on-demand result -----------------------------------------------

export const RecordResultSchema = z.object({
  recorded: z.boolean(),
  /** "string" | "json" | "binary" | "bool" | "number" */
  kind: z.string(),
});

// ---- inferred types --------------------------------------------------------

export type HistoryRecord = z.infer<typeof HistoryRecordSchema>;
export type ScalarRecord = z.infer<typeof ScalarRecordSchema>;
export type RecordResult = z.infer<typeof RecordResultSchema>;
export type BucketedRow = z.infer<typeof BucketedRowSchema>;
export type BucketedMeta = z.infer<typeof BucketedMetaSchema>;
export type BucketedTelemetryResponse = z.infer<
  typeof BucketedTelemetryResponseSchema
>;
export type HistoryBucketedRow = z.infer<typeof HistoryBucketedRowSchema>;
export type HistoryBucketedMeta = z.infer<typeof HistoryBucketedMetaSchema>;
export type HistoryBucketedResponse = z.infer<
  typeof HistoryBucketedResponseSchema
>;
export type GroupedSeries = z.infer<typeof GroupedSeriesSchema>;
export type GroupedTelemetryMeta = z.infer<typeof GroupedTelemetryMetaSchema>;
export type GroupedTelemetryResponse = z.infer<
  typeof GroupedTelemetryResponseSchema
>;
