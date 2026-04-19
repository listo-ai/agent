import type { ClientError } from "../errors.js";

export interface HttpClientOptions {
  baseUrl: string;
  /** Optional bearer token. */
  token?: string;
  /** Request timeout in ms. Default: 10_000. */
  timeoutMs?: number;
}

// Build common request headers.
function buildHeaders(token: string | undefined): HeadersInit {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    Accept: "application/json",
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }
  return headers;
}

// Raise a structured ClientError from a non-ok Response.
async function toClientError(res: Response): Promise<ClientError> {
  let body: string | undefined;
  try { body = await res.text(); } catch { /* ignore */ }
  return {
    kind: "HttpError",
    status: res.status,
    message: body ?? res.statusText,
  };
}

export class HttpClient {
  private readonly opts: Required<HttpClientOptions>;

  constructor(opts: HttpClientOptions) {
    this.opts = {
      baseUrl: opts.baseUrl.replace(/\/$/, ""),
      token: opts.token ?? "",
      timeoutMs: opts.timeoutMs ?? 10_000,
    };
  }

  async get<T>(path: string): Promise<T> {
    const res = await fetch(`${this.opts.baseUrl}${path}`, {
      method: "GET",
      headers: buildHeaders(this.opts.token),
      signal: AbortSignal.timeout(this.opts.timeoutMs),
    });
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }

  async post<T>(path: string, body: unknown): Promise<T> {
    const res = await fetch(`${this.opts.baseUrl}${path}`, {
      method: "POST",
      headers: buildHeaders(this.opts.token),
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(this.opts.timeoutMs),
    });
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }

  async put<T>(path: string, body: unknown): Promise<T> {
    const res = await fetch(`${this.opts.baseUrl}${path}`, {
      method: "PUT",
      headers: buildHeaders(this.opts.token),
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(this.opts.timeoutMs),
    });
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }
}
