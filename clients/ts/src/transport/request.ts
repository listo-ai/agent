/**
 * RequestTransport — the minimal interface that every domain module
 * (`createNodesApi`, `createSlotsApi`, …) depends on.
 *
 * Two implementations exist:
 *   - `HttpClient`            — direct `fetch` against the local agent.
 *   - `FleetRequestTransport` — routes each call through a fleet
 *                               req/reply round-trip to a remote agent.
 *
 * Nothing in `domain/` imports a concrete class; callers compose the
 * right transport and inject it. The dependency arrow is:
 *   `client.ts` → `RequestTransport` ← `domain/*`
 *                 ↑              ↑
 *          `HttpClient`  `FleetRequestTransport`
 */
export interface RequestTransport {
  get<T>(path: string): Promise<T>;
  post<T>(path: string, body: unknown): Promise<T>;
  /** POST that expects a 204 / empty body. */
  postNoContent(path: string, body: unknown): Promise<void>;
  put<T>(path: string, body: unknown): Promise<T>;
  delete(path: string): Promise<void>;
}

/**
 * One fleet request/reply call from the TypeScript side.
 *
 * The caller (Studio or a test fixture) owns both the fleet WS/NATS
 * connection and the subject-building logic (it knows the backend's
 * subject format). `FleetRequestTransport` receives this function via
 * constructor injection and calls it for every domain operation.
 *
 * @param subject  Fully-qualified fleet subject, e.g.
 *                 `"fleet.sys.edge-42.api.v1.nodes.list"`.
 * @param body     JSON-serialisable request body (can be `undefined`
 *                 for reads).
 * @param timeoutMs  Request deadline in milliseconds.
 * @returns        Parsed JSON response body — the same shape the
 *                 equivalent REST endpoint would return.
 */
export type FleetRequestFn = (
  subject: string,
  body: unknown,
  timeoutMs: number,
) => Promise<unknown>;

/**
 * Build a fleet subject from a REST path, method, and scope prefix.
 *
 * Convention matches what `transport_rest::fleet::mount` registers on
 * the Rust side — each route is wired to a subject derived from its
 * path + HTTP verb:
 *
 * ```
 * GET  /api/v1/nodes          →  <prefix>.api.v1.nodes.list
 * GET  /api/v1/node?path=...  →  <prefix>.api.v1.node.get
 * POST /api/v1/nodes          →  <prefix>.api.v1.nodes.create
 * DELETE /api/v1/node?...     →  <prefix>.api.v1.node.remove
 * POST /api/v1/slots          →  <prefix>.api.v1.slots.write
 * POST /api/v1/config         →  <prefix>.api.v1.config.set
 * POST /api/v1/lifecycle      →  <prefix>.api.v1.lifecycle.transition
 * POST /api/v1/seed           →  <prefix>.api.v1.seed.apply
 * GET  /api/v1/blocks        →  <prefix>.api.v1.blocks.list
 * GET  /api/v1/blocks/:id    →  <prefix>.api.v1.block.get
 * POST /api/v1/blocks/:id/enable   →  <prefix>.api.v1.block.enable
 * POST /api/v1/blocks/:id/disable  →  <prefix>.api.v1.block.disable
 * POST /api/v1/blocks/reload →  <prefix>.api.v1.blocks.reload
 * GET  /api/v1/kinds          →  <prefix>.api.v1.kinds.list
 * GET  /api/v1/links          →  <prefix>.api.v1.links.list
 * POST /api/v1/links          →  <prefix>.api.v1.links.create
 * DELETE /api/v1/links/:id    →  <prefix>.api.v1.link.remove
 * GET  /api/v1/auth/whoami    →  <prefix>.api.v1.auth.whoami
 * GET  /api/v1/ui/nav         →  <prefix>.api.v1.ui.nav
 * POST /api/v1/ui/resolve     →  <prefix>.api.v1.ui.resolve
 * GET  /api/v1/capabilities   →  <prefix>.api.v1.capabilities
 * ```
 *
 * The mapping is intentionally explicit — no auto-derive from path
 * segments — because the Rust side registers each subject by hand and
 * an undeclared path should be a loud compile-time error, not a silent
 * runtime miss.
 */
export function pathToSubject(
  prefix: string,
  method: "GET" | "POST" | "PUT" | "DELETE",
  path: string,
): string {
  // Strip query string for routing; query params ride in the body payload.
  const bare = path.split("?")[0]!.replace(/\/$/, "");

  // Lookup table: `METHOD bare-path` → subject suffix (without prefix).
  const table: Record<string, string> = {
    "GET /api/v1/nodes":                   "api.v1.nodes.list",
    "GET /api/v1/node":                    "api.v1.node.get",
    "POST /api/v1/nodes":                  "api.v1.nodes.create",
    "DELETE /api/v1/node":                 "api.v1.node.remove",
    "POST /api/v1/slots":                  "api.v1.slots.write",
    "POST /api/v1/config":                 "api.v1.config.set",
    "POST /api/v1/lifecycle":              "api.v1.lifecycle.transition",
    "POST /api/v1/seed":                   "api.v1.seed.apply",
    "GET /api/v1/blocks":                 "api.v1.blocks.list",
    "POST /api/v1/blocks/reload":         "api.v1.blocks.reload",
    "GET /api/v1/kinds":                   "api.v1.kinds.list",
    "GET /api/v1/links":                   "api.v1.links.list",
    "POST /api/v1/links":                  "api.v1.links.create",
    "GET /api/v1/auth/whoami":             "api.v1.auth.whoami",
    "GET /api/v1/ui/nav":                  "api.v1.ui.nav",
    "POST /api/v1/ui/resolve":             "api.v1.ui.resolve",
    "GET /api/v1/capabilities":            "api.v1.capabilities",
  };

  // Dynamic routes: block get/enable/disable, link delete.
  // e.g. GET /api/v1/blocks/com_acme_hello → api.v1.block.get
  const dynamic: Array<[RegExp, string, string]> = [
    [/^\/api\/v\d+\/blocks\/[^/]+\/enable$/, "POST",   "api.v1.block.enable"],
    [/^\/api\/v\d+\/blocks\/[^/]+\/disable$/, "POST",  "api.v1.block.disable"],
    [/^\/api\/v\d+\/blocks\/[^/]+$/, "GET",            "api.v1.block.get"],
    [/^\/api\/v\d+\/links\/[^/]+$/, "DELETE",           "api.v1.link.remove"],
    [/^\/api\/v\d+\/kinds/, "GET",                      "api.v1.kinds.list"],
  ];

  const key = `${method} ${bare}`;
  const suffix = table[key] ?? dynamic.find(([re, m]) => m === method && re.test(bare))?.[2];

  if (!suffix) {
    throw new Error(
      `FleetRequestTransport: no fleet subject registered for ${method} ${path}. ` +
        "Add an entry to pathToSubject() and the corresponding Rust fleet handler.",
    );
  }

  return `${prefix}.${suffix}`;
}
