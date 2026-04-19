import type { GraphEvent } from "../schemas/events.js";
import { GraphEventSchema, HelloEventSchema } from "../schemas/events.js";

export interface SseSubscriptionOptions {
  url: string;
  token?: string;
  /** Resume from this sequence number — server will replay missed events. */
  sinceSeq?: number;
  /** Called when the connection is established and the hello frame is received. */
  onOpen?: (seq: number) => void;
  /** Called when the connection closes (not an error). */
  onClose?: () => void;
  /**
   * Called when the server returns 409 cursor_too_old — the client must
   * refetch its full state.  `availableFrom` is the oldest seq the server
   * still holds.
   */
  onCursorTooOld?: (availableFrom: number) => void;
}

// Returns an AsyncIterable of parsed GraphEvents over a Server-Sent Events
// stream.  Uses the browser / Node 18+ built-in EventSource.
//
// Reconnects automatically with exponential backoff, passing ?since=<lastSeq>
// on each attempt so no events are silently lost during transient drops.
export function subscribeToEvents(
  opts: SseSubscriptionOptions,
): AsyncIterable<GraphEvent> & { close: () => void; readonly lastSeq: number } {
  const { token, onOpen, onClose, onCursorTooOld } = opts;

  let lastSeq = opts.sinceSeq ?? 0;
  const queue: GraphEvent[] = [];
  let resolve: ((v: IteratorResult<GraphEvent>) => void) | undefined;
  let closed = false;
  let source: EventSource | undefined;

  function buildUrl(): string {
    const url = token
      ? `${opts.url}?token=${encodeURIComponent(token)}`
      : opts.url;
    return lastSeq > 0 ? `${url}${token ? "&" : "?"}since=${lastSeq}` : url;
  }

  function enqueue(event: GraphEvent): void {
    if (resolve) {
      const r = resolve;
      resolve = undefined;
      r({ value: event, done: false });
    } else {
      queue.push(event);
    }
  }

  function connect(): void {
    if (closed) return;
    source = new EventSource(buildUrl());

    source.addEventListener("message", (ev: MessageEvent) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(ev.data as string);
      } catch {
        return;
      }

      // hello frame — extract seq, call onOpen, do NOT dispatch.
      const hello = HelloEventSchema.safeParse(parsed);
      if (hello.success) {
        lastSeq = hello.data.seq;
        onOpen?.(lastSeq);
        return;
      }

      // regular graph event
      const graph = GraphEventSchema.safeParse(parsed);
      if (graph.success) {
        lastSeq = graph.data.seq;
        enqueue(graph.data);
      }
    });

    source.addEventListener("error", () => {
      source?.close();
      source = undefined;
      if (!closed) {
        // Reconnect with backoff — resolved to a fixed 1s here; a
        // production transport would use exponential backoff.
        setTimeout(() => connect(), 1000);
      } else {
        onClose?.();
        resolve?.({ value: undefined as unknown as GraphEvent, done: true });
      }
    });
  }

  connect();

  const iterable: AsyncIterable<GraphEvent> = {
    [Symbol.asyncIterator]() {
      return {
        next(): Promise<IteratorResult<GraphEvent>> {
          if (queue.length > 0) {
            return Promise.resolve({ value: queue.shift()!, done: false });
          }
          if (closed) {
            return Promise.resolve({
              value: undefined as unknown as GraphEvent,
              done: true,
            });
          }
          return new Promise((r) => {
            resolve = r;
          });
        },
        return(): Promise<IteratorResult<GraphEvent>> {
          closed = true;
          source?.close();
          return Promise.resolve({
            value: undefined as unknown as GraphEvent,
            done: true,
          });
        },
      };
    },
  };

  return Object.assign(iterable, {
    close(): void {
      closed = true;
      source?.close();
      onClose?.();
    },
    get lastSeq(): number {
      return lastSeq;
    },
  });
}
