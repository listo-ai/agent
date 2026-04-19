import type { ClientError } from "../errors.js";

export interface HttpClientOptions {
  baseUrl: string;
  /** Optional bearer token. */
  token?: string;
  /** Request timeout in ms. Default: 10_000. */
  timeoutMs?: number;
}

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

async function toClientError(res: Response): Promise<ClientError> {
  let body: string | undefined;
  try {
    body = await res.text();
  } catch {
    /* ignore */
  }
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

  private request(
    method: "GET" | "POST" | "PUT" | "DELETE",
    path: string,
    body?: unknown,
  ): Promise<Response> {
    const init: RequestInit = {
      method,
      headers: buildHeaders(this.opts.token),
      signal: AbortSignal.timeout(this.opts.timeoutMs),
    };
    if (body !== undefined) {
      init.body = JSON.stringify(body);
    }
    return fetch(`${this.opts.baseUrl}${path}`, init);
  }

  async get<T>(path: string): Promise<T> {
    const res = await this.request("GET", path);
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }

  async post<T>(path: string, body: unknown): Promise<T> {
    const res = await this.request("POST", path, body);
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }

  /** POST variant for endpoints that respond with 204 / empty body. */
  async postNoContent(path: string, body: unknown): Promise<void> {
    const res = await this.request("POST", path, body);
    if (!res.ok) throw await toClientError(res);
  }

  async put<T>(path: string, body: unknown): Promise<T> {
    const res = await this.request("PUT", path, body);
    if (!res.ok) throw await toClientError(res);
    return res.json() as Promise<T>;
  }

  async delete(path: string): Promise<void> {
    const res = await this.request("DELETE", path);
    if (!res.ok) throw await toClientError(res);
  }
}
