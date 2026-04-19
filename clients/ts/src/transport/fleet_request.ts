import type { RequestTransport, FleetRequestFn } from "./request.js";
import { pathToSubject } from "./request.js";
import type { ClientError } from "../errors.js";

/**
 * Fleet-backed implementation of `RequestTransport`.
 *
 * Each domain call (e.g. `nodes.list()`) becomes a fleet req/reply
 * round-trip to a remote agent:
 *
 *   domain call  →  `FleetRequestTransport`
 *                →  `fleetRequestFn(subject, body, timeoutMs)`
 *                →  caller-supplied fleet WS/NATS connection
 *                →  remote agent's fleet handler
 *                →  JSON response
 *
 * The caller owns the fleet connection and the subject-building logic.
 * This transport only knows three things: the fleet subject prefix for
 * the target agent, the timeout, and how to map a REST path to a fleet
 * subject via `pathToSubject()`.
 *
 * The response envelope coming back from the remote handler mirrors the
 * HTTP surface exactly — same JSON shapes, same error format. Errors
 * returned as `{ error: { code, message } }` in the reply payload are
 * surfaced as `HttpError` client errors to keep the call-site error
 * handling uniform.
 */
export class FleetRequestTransport implements RequestTransport {
  private readonly subjectPrefix: string;
  private readonly requestFn: FleetRequestFn;
  private readonly timeoutMs: number;

  /**
   * @param subjectPrefix  e.g. `"fleet.sys.edge-42"` — the
   *   `fleet.<tenant>.<agent-id>` portion.  `pathToSubject` appends
   *   the route-specific suffix.
   * @param requestFn  Fleet req/reply callable provided by the caller.
   * @param timeoutMs  Per-request deadline. Default: 10_000.
   */
  constructor(
    subjectPrefix: string,
    requestFn: FleetRequestFn,
    timeoutMs = 10_000,
  ) {
    this.subjectPrefix = subjectPrefix;
    this.requestFn = requestFn;
    this.timeoutMs = timeoutMs;
  }

  private async call<T>(
    method: "GET" | "POST" | "PUT" | "DELETE",
    path: string,
    body?: unknown,
  ): Promise<T> {
    const subject = pathToSubject(this.subjectPrefix, method, path);

    // Carry query-string params in the body envelope so the fleet
    // handler can unpack them alongside the real body.
    const queryString = path.includes("?") ? path.split("?")[1] : undefined;
    const payload: Record<string, unknown> = {};
    if (queryString) {
      payload["_qs"] = queryString;
    }
    if (body !== undefined) {
      payload["_body"] = body;
    }

    const raw = await this.requestFn(
      subject,
      Object.keys(payload).length > 0 ? payload : undefined,
      this.timeoutMs,
    );

    // Check for error envelope from the remote handler.
    if (
      raw !== null &&
      typeof raw === "object" &&
      "error" in (raw as object)
    ) {
      const err = (raw as { error: { code?: number; message?: string } }).error;
      const clientError: ClientError = {
        kind: "HttpError",
        status: err.code ?? 500,
        message: err.message ?? "fleet handler returned an error",
      };
      throw clientError;
    }

    return raw as T;
  }

  async get<T>(path: string): Promise<T> {
    return this.call<T>("GET", path);
  }

  async post<T>(path: string, body: unknown): Promise<T> {
    return this.call<T>("POST", path, body);
  }

  async postNoContent(path: string, body: unknown): Promise<void> {
    await this.call<unknown>("POST", path, body);
  }

  async put<T>(path: string, body: unknown): Promise<T> {
    return this.call<T>("PUT", path, body);
  }

  async delete(path: string): Promise<void> {
    await this.call<unknown>("DELETE", path);
  }
}
