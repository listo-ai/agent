import { z } from "zod";

/** `GET /api/v1/ai/providers` — one entry per registered runner. */
export const AiProviderStatusSchema = z.object({
  provider: z.string(),
  available: z.boolean(),
});
export type AiProviderStatus = z.infer<typeof AiProviderStatusSchema>;

/** Request body for `POST /api/v1/ai/run`. */
export const AiRunRequestSchema = z.object({
  prompt: z.string(),
  system_prompt: z.string().optional(),
  provider: z.string().optional(),
  model: z.string().optional(),
  max_tokens: z.number().int().nonnegative().optional(),
  /** Extended thinking / reasoning effort: `low` | `medium` | `high` | `off` |
   *  raw token budget as a string. */
  thinking_budget: z.string().optional(),
});
export type AiRunRequest = z.infer<typeof AiRunRequestSchema>;

/** Response from `POST /api/v1/ai/run`. */
export const AiRunResponseSchema = z.object({
  text: z.string(),
  provider: z.string(),
  model: z.string().optional(),
  input_tokens: z.number().int().nonnegative(),
  output_tokens: z.number().int().nonnegative(),
  duration_ms: z.number().int().nonnegative(),
});
export type AiRunResponse = z.infer<typeof AiRunResponseSchema>;

/**
 * One SSE frame from `POST /api/v1/ai/stream`. Tagged by `type` — matches
 * `ai_runner::EventKind` wire-flat plus a terminal `result` variant that
 * always arrives last (even on errors, which emit `error` then `result`).
 */
export const AiStreamEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("connected"), model: z.string().nullable().optional() }),
  z.object({ type: z.literal("text"), content: z.string() }),
  z.object({ type: z.literal("tool_call"), name: z.string() }),
  z.object({ type: z.literal("tool_use"), id: z.string(), name: z.string(), input: z.unknown() }),
  z.object({
    type: z.literal("done"),
    duration_ms: z.number().int().nonnegative(),
    cost_usd: z.number(),
    input_tokens: z.number().int().nonnegative(),
    output_tokens: z.number().int().nonnegative(),
  }),
  z.object({ type: z.literal("error"), message: z.string() }),
  z.object({
    type: z.literal("result"),
    text: z.string(),
    provider: z.string(),
    model: z.string().nullable().optional(),
    input_tokens: z.number().int().nonnegative(),
    output_tokens: z.number().int().nonnegative(),
    duration_ms: z.number().int().nonnegative(),
  }),
]);
export type AiStreamEvent = z.infer<typeof AiStreamEventSchema>;
