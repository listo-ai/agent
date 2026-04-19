import type { SseSubscriptionOptions } from "../transport/sse.js";
import { subscribeToEvents } from "../transport/sse.js";
import type { GraphEvent } from "../schemas/events.js";

export interface EventsApi {
  /**
   * Subscribe to graph events. Returns an AsyncIterable you iterate
   * with `for await`, plus `close()` and a `lastSeq` getter for
   * reconnect cursors.
   *
   * `sinceSeq` resumes from a previous cursor — the server will replay
   * any events since that point before streaming live events.
   */
  subscribe(
    opts?: Partial<
      Pick<SseSubscriptionOptions, "onOpen" | "onClose" | "sinceSeq" | "onCursorTooOld">
    >,
  ): AsyncIterable<GraphEvent> & { close: () => void; readonly lastSeq: number };
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
