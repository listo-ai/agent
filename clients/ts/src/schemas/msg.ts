import { z } from "zod";

// Node-RED-compatible Msg envelope.
// Immutable on the wire; the agent never mutates a received Msg.
export const MsgSchema = z.object({
  /** Unique message id — agent-assigned, opaque string. */
  _msgid: z.string(),
  /** Routing topic, e.g. "node.value.changed". */
  topic: z.string().optional(),
  /** Arbitrary payload — number, string, object, array, null. */
  payload: z.unknown(),
  /** ISO-8601 timestamp assigned by the emitting node. */
  ts: z.string().optional(),
});

export type Msg = z.infer<typeof MsgSchema>;
