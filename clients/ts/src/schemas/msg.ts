import { z } from "zod";

/**
 * Node-RED-compatible message envelope.
 *
 * Wire layout mirrors the Rust `spi::Msg` type exactly — see
 * `crates/spi/src/msg.rs`. Underscore-prefixed keys are platform-reserved;
 * any other root-level key is a user-added custom field.
 */
export const MsgSchema = z
  .object({
    /** Primary data. Same semantics as Node-RED's `msg.payload`. */
    payload: z.unknown(),
    /** Routing / grouping hint. Same semantics as Node-RED's `msg.topic`. */
    topic: z.string().optional(),
    /** Message identifier. UUID string, serialised as `_msgid` for Node-RED parity. */
    _msgid: z.string(),
    /** Parent message id when this message was derived from another. */
    _parentid: z.string().optional(),
    /** Creation timestamp, milliseconds since Unix epoch. */
    _ts: z.number().int().nonnegative(),
    /** Emitting node's path in the graph, if known. */
    _source: z.string().optional(),
  })
  .passthrough(); // Custom user fields flatten onto the root object.

export type Msg = z.infer<typeof MsgSchema>;
