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
