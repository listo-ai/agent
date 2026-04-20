/**
 * Dot-path extraction for widget `source.field`.
 *
 * After docs/design/NODE-RED-MODEL.md Stage 4, output slots hold a full
 * `Msg` envelope `{ _msgid, payload: { … } }`. Widgets that want a
 * numeric or boolean scalar set `source.field: "payload.count"` to
 * reach into the envelope. Widgets that leave `field` unset fall back
 * to a single-level `.payload` auto-unwrap so pre-Stage-5 authored
 * pages keep rendering.
 */
export function extractField(v: unknown, field?: string): unknown {
  if (field && field.length > 0) {
    let cursor: unknown = v;
    for (const segment of field.split(".")) {
      if (cursor && typeof cursor === "object" && !Array.isArray(cursor) && segment in cursor) {
        cursor = (cursor as Record<string, unknown>)[segment];
      } else {
        return undefined;
      }
    }
    return cursor;
  }
  // No field → legacy auto-unwrap of Msg envelopes.
  if (v && typeof v === "object" && !Array.isArray(v) && "payload" in v) {
    return (v as { payload: unknown }).payload;
  }
  return v;
}
