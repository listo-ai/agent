import type { RequestTransport } from "../transport/request.js";
import { streamSsePost } from "../transport/sse-post.js";
import {
  AiProviderStatusSchema,
  AiRunResponseSchema,
  AiStreamEventSchema,
  type AiProviderStatus,
  type AiRunRequest,
  type AiRunResponse,
  type AiStreamEvent,
} from "../schemas/ai.js";

export interface AiStreamOptions {
  /** AbortSignal to cancel the stream (component unmount, user cancel). */
  signal?: AbortSignal | undefined;
}

export interface AiApi {
  /** List registered providers with their availability flags. */
  providers(): Promise<AiProviderStatus[]>;
  /** Run a one-shot prompt through the shared registry. */
  run(req: AiRunRequest): Promise<AiRunResponse>;
  /**
   * Stream a prompt. Yields one [`AiStreamEvent`] per frame; the last
   * one is always `{ type: "result", ... }` (`error` arrives just
   * before it when the backend fails). Cancel via `opts.signal`.
   */
  stream(req: AiRunRequest, opts?: AiStreamOptions): AsyncGenerator<AiStreamEvent, void, void>;
}

export function createAiApi(
  http: RequestTransport,
  apiVersion: number,
  /** Only used by `stream()` — fetch is direct, not via the transport. */
  streamCtx?: { baseUrl: string; token?: string | undefined },
): AiApi {
  const base = `/api/v${apiVersion}/ai`;

  return {
    async providers(): Promise<AiProviderStatus[]> {
      const raw = await http.get<unknown[]>(`${base}/providers`);
      return raw.map((entry) => AiProviderStatusSchema.parse(entry));
    },

    async run(req: AiRunRequest): Promise<AiRunResponse> {
      const raw = await http.post<unknown>(`${base}/run`, req);
      return AiRunResponseSchema.parse(raw);
    },

    async *stream(
      req: AiRunRequest,
      opts?: AiStreamOptions,
    ): AsyncGenerator<AiStreamEvent, void, void> {
      if (!streamCtx) {
        throw new Error(
          "AiApi.stream requires a local baseUrl — fleet-scoped clients cannot stream yet.",
        );
      }
      const url = `${streamCtx.baseUrl}${base}/stream`;
      const frames = streamSsePost({
        url,
        body: req,
        ...(streamCtx.token !== undefined && { token: streamCtx.token }),
        ...(opts?.signal !== undefined && { signal: opts.signal }),
      });
      for await (const raw of frames) {
        let parsed: unknown;
        try {
          parsed = JSON.parse(raw);
        } catch {
          continue;
        }
        const res = AiStreamEventSchema.safeParse(parsed);
        if (res.success) yield res.data;
      }
    },
  };
}
