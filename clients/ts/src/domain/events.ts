import type { SseSubscriptionOptions } from "../transport/sse.js";
import { subscribeToEvents } from "../transport/sse.js";
import type { GraphEvent } from "../schemas/events.js";

export interface EventsApi {
  /** Subscribe to graph events. Returns an AsyncIterable you iterate with `for await`. */
  subscribe(
    opts?: Partial<Pick<SseSubscriptionOptions, "onOpen" | "onClose">>,
  ): AsyncIterable<GraphEvent> & { close: () => void };
}

export function createEventsApi(
  baseUrl: string,
  apiVersion: number,
  token: string | undefined,
): EventsApi {
  return {
    subscribe(opts = {}) {
      return subscribeToEvents({
        url: `${baseUrl}/api/v${apiVersion}/events`,
        ...(token !== undefined && { token }),
        ...opts,
      });
    },
  };
}
