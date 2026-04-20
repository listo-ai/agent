import { HttpClient } from "./transport/http.js";
import { FleetRequestTransport } from "./transport/fleet_request.js";
import type { FleetRequestFn, RequestTransport } from "./transport/request.js";
import { createNodesApi } from "./domain/nodes.js";
import { createSlotsApi } from "./domain/slots.js";
import { createConfigApi } from "./domain/config.js";
import { createEventsApi } from "./domain/events.js";
import { createCapabilitiesApi } from "./domain/capabilities.js";
import { createLinksApi } from "./domain/links.js";
import { createLifecycleApi } from "./domain/lifecycle.js";
import { createSeedApi } from "./domain/seed.js";
import { createPluginsApi } from "./domain/blocks.js";
import { createKindsApi } from "./domain/kinds.js";
import { createAuthApi } from "./domain/auth.js";
import { createUiApi } from "./domain/ui.js";
import { createHistoryApi } from "./domain/history.js";
import type { NodesApi } from "./domain/nodes.js";
import type { SlotsApi } from "./domain/slots.js";
import type { HistoryApi } from "./domain/history.js";
import type { ConfigApi } from "./domain/config.js";
import type { EventsApi } from "./domain/events.js";
import type { LinksApi } from "./domain/links.js";
import type { LifecycleApi } from "./domain/lifecycle.js";
import type { SeedApi } from "./domain/seed.js";
import type { PluginsApi } from "./domain/blocks.js";
import type { KindsApi } from "./domain/kinds.js";
import type { AuthApi } from "./domain/auth.js";
import type { UiApi } from "./domain/ui.js";
import { REST_API_VERSION } from "./version.js";
import { FleetScope } from "./schemas/fleet.js";
import type { FleetScope as FleetScopeType } from "./schemas/fleet.js";

export interface AgentClientOptions {
  /** Base URL of the agent, e.g. "http://localhost:8080". */
  baseUrl: string;
  /** Optional bearer token for authenticated agents. */
  token?: string;
  /** Request timeout in ms. Default: 10_000. */
  timeoutMs?: number;
  /**
   * Skip capability check on connect.
   * Only for testing against agents that don't yet expose /capabilities.
   */
  skipCapabilityCheck?: boolean;
  /**
   * Scope for this client. Defaults to `FleetScope.local()`.
   *
   * When `FleetScope.remote(tenant, agentId)`: every domain call routes
   * through `fleetRequestFn` instead of `fetch`. `events.subscribe()`
   * returns a no-op iterator — fleet graph-event subscriptions for a
   * remote agent are managed via `FleetScope.subscribeEvents()` on the
   * caller's fleet connection, not through `AgentClient.events`.
   */
  scope?: FleetScopeType;
  /**
   * Required when `scope` is `FleetScope.remote(…)`.
   *
   * The caller provides this function from their fleet WS/NATS
   * connection. It receives the fully-qualified fleet subject and the
   * request body, performs a req/reply round-trip, and returns the
   * parsed JSON response.
   *
   * The subject prefix (`fleet.<tenant>.<agent-id>`) is derived from
   * `scope`; the suffix is mapped from the REST path by
   * `pathToSubject()` in `transport/request.ts`.
   */
  fleetRequestFn?: FleetRequestFn;
}

/**
 * AgentClient — the public facade.
 *
 * Construct with `await AgentClient.connect(opts)`. The constructor is
 * private; `connect` performs the capability handshake before
 * returning.
 *
 * Dependency direction: client → domain → RequestTransport → schemas.
 * domain/* never imports fetch directly; transport/* never imports domain.
 *
 * Two transports are available:
 *   - `HttpClient`            — default; `fetch` against `baseUrl`.
 *   - `FleetRequestTransport` — used when `scope` is `Remote`.
 */
export class AgentClient {
  /** Which agent this client is targeting. */
  readonly scope: FleetScopeType;

  readonly nodes: NodesApi;
  readonly slots: SlotsApi;
  readonly config: ConfigApi;
  readonly events: EventsApi;
  readonly links: LinksApi;
  readonly lifecycle: LifecycleApi;
  readonly seed: SeedApi;
  readonly blocks: PluginsApi;
  readonly kinds: KindsApi;
  readonly auth: AuthApi;
  readonly ui: UiApi;
  readonly history: HistoryApi;

  private constructor(
    transport: RequestTransport,
    opts: AgentClientOptions,
    scope: FleetScopeType,
  ) {
    this.scope = scope;
    this.nodes = createNodesApi(transport, REST_API_VERSION);
    this.slots = createSlotsApi(transport, REST_API_VERSION);
    this.config = createConfigApi(transport, REST_API_VERSION);
    this.links = createLinksApi(transport, REST_API_VERSION);
    this.lifecycle = createLifecycleApi(transport, REST_API_VERSION);
    this.seed = createSeedApi(transport, REST_API_VERSION);
    this.blocks = createPluginsApi(transport, REST_API_VERSION);
    this.kinds = createKindsApi(transport, REST_API_VERSION);
    this.auth = createAuthApi(transport, REST_API_VERSION);
    this.ui = createUiApi(transport, REST_API_VERSION);
    this.history = createHistoryApi(transport, REST_API_VERSION);

    if (FleetScope.isLocal(scope)) {
      // Local: real SSE stream against baseUrl.
      this.events = createEventsApi(opts.baseUrl, REST_API_VERSION, opts.token);
    } else {
      // Remote: fleet graph-event subscriptions are managed by the
      // caller on their fleet connection, not through AgentClient.events.
      // Return a no-op that completes immediately so call sites don't
      // break — they should not use AgentClient.events for remote scopes.
      this.events = makeRemoteEventsStub();
    }
  }

  /**
   * Create a connected AgentClient.
   *
   * When `opts.scope` is `FleetScope.remote(…)`, `opts.fleetRequestFn`
   * must also be supplied. The capability check runs against the remote
   * agent via fleet req/reply in that case.
   */
  static async connect(opts: AgentClientOptions): Promise<AgentClient> {
    const scope = opts.scope ?? FleetScope.local();
    const timeoutMs = opts.timeoutMs ?? 10_000;

    let transport: RequestTransport;

    if (FleetScope.isLocal(scope)) {
      transport = new HttpClient({
        baseUrl: opts.baseUrl,
        ...(opts.token !== undefined && { token: opts.token }),
        timeoutMs,
      });
    } else {
      // Remote scope — fleet req/reply.
      if (!opts.fleetRequestFn) {
        throw new Error(
          "AgentClient.connect: `fleetRequestFn` is required when scope is Remote. " +
            "Provide a fleet request function from your fleet WS/NATS connection.",
        );
      }
      const subjectPrefix = `fleet.${scope.tenant}.${scope.agent_id}`;
      transport = new FleetRequestTransport(
        subjectPrefix,
        opts.fleetRequestFn,
        timeoutMs,
      );
    }

    const client = new AgentClient(transport, opts, scope);

    if (!opts.skipCapabilityCheck) {
      const capsApi = createCapabilitiesApi(transport);
      const manifest = await capsApi.getManifest();
      capsApi.assertRequirements(manifest);
    }

    return client;
  }
}

/** EventsApi stub returned for remote-scoped clients. */
function makeRemoteEventsStub(): EventsApi {
  return {
    subscribe() {
      let _closed = false;
      const iterable: AsyncIterable<never> & {
        close: () => void;
        readonly lastSeq: number;
      } = {
        lastSeq: 0,
        close() {
          _closed = true;
        },
        [Symbol.asyncIterator]() {
          return {
            async next() {
              return { value: undefined as never, done: true };
            },
            async return() {
              _closed = true;
              return { value: undefined as never, done: true };
            },
          };
        },
      };
      return iterable;
    },
  };
}

