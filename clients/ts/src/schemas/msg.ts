import { z } from "zod";

/**
 * Node-RED-compatible message envelope.
 *
 * Wire layout mirrors the Rust `spi::Msg` type exactly — see
 * `crates/spi/src/msg.rs` and `docs/design/NODE-RED-MODEL.md`.
 * Three fields only: `payload`, `topic`, `_msgid`. Any other
 * root-level key is a user-added custom field. Stage 2 stripped
 * `_ts`, `_source`, `_parentid` — timestamps now ride the SSE
 * frame / history writer, source is derived from the subject,
 * parent lineage carries as W3C TraceContext in transport metadata.
 */
export const MsgSchema = z
  .object({
    /** Primary data. Same semantics as Node-RED's `msg.payload`. */
    payload: z.unknown(),
    /** Routing / grouping hint. Same semantics as Node-RED's `msg.topic`. */
    topic: z.string().optional(),
    /** Message identifier. UUID string, serialised as `_msgid` for Node-RED parity. */
    _msgid: z.string(),
  })
  .passthrough(); // Custom user fields + legacy producers' _ts/_source/_parentid flatten onto the root.

export type Msg = z.infer<typeof MsgSchema>;
