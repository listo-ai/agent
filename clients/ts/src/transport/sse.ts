import type { GraphEvent } from "../schemas/events.js";
import { GraphEventSchema } from "../schemas/events.js";

export interface SseSubscriptionOptions {
  url: string;
  token?: string;
  /** Called when the connection is established. */
  onOpen?: () => void;
  /** Called when the connection closes (not an error). */
  onClose?: () => void;
}

// Returns an AsyncIterable of parsed GraphEvents over a Server-Sent Events stream.
// Uses the browser / Node 18+ built-in EventSource.
export function subscribeToEvents(
  opts: SseSubscriptionOptions,
): AsyncIterable<GraphEvent> & { close: () => void } {
  const url = opts.token
    ? `${opts.url}?token=${encodeURIComponent(opts.token)}`
    : opts.url;

  const source = new EventSource(url);
  const queue: GraphEvent[] = [];
  let resolve: ((v: IteratorResult<GraphEvent>) => void) | undefined;
  let closed = false;

  function enqueue(event: GraphEvent): void {
    if (resolve) {
      const r = resolve;
      resolve = undefined;
      r({ value: event, done: false });
    } else {
      queue.push(event);
    }
  }

  source.addEventListener("open", () => opts.onOpen?.());

  source.addEventListener("message", (ev: MessageEvent) => {
    const parsed = GraphEventSchema.safeParse(JSON.parse(ev.data as string));
    if (parsed.success) enqueue(parsed.data);
  });

  source.addEventListener("error", () => {
    closed = true;
    source.close();
    opts.onClose?.();
    resolve?.({ value: undefined as unknown as GraphEvent, done: true });
  });

  const iterable: AsyncIterable<GraphEvent> = {
    [Symbol.asyncIterator]() {
      return {
        next(): Promise<IteratorResult<GraphEvent>> {
          if (queue.length > 0) {
            return Promise.resolve({ value: queue.shift()!, done: false });
          }
          if (closed) {
            return Promise.resolve({ value: undefined as unknown as GraphEvent, done: true });
          }
          return new Promise((r) => { resolve = r; });
        },
        return(): Promise<IteratorResult<GraphEvent>> {
          closed = true;
          source.close();
          return Promise.resolve({ value: undefined as unknown as GraphEvent, done: true });
        },
      };
    },
  };

  return Object.assign(iterable, {
    close(): void {
      closed = true;
      source.close();
      opts.onClose?.();
    },
  });
}
