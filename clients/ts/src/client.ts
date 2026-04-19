import { HttpClient } from "./transport/http.js";
import { createNodesApi } from "./domain/nodes.js";
import { createSlotsApi } from "./domain/slots.js";
import { createConfigApi } from "./domain/config.js";
import { createEventsApi } from "./domain/events.js";
import { createCapabilitiesApi } from "./domain/capabilities.js";
import { createLinksApi } from "./domain/links.js";
import { createLifecycleApi } from "./domain/lifecycle.js";
import { createSeedApi } from "./domain/seed.js";
import { createPluginsApi } from "./domain/plugins.js";
import { createKindsApi } from "./domain/kinds.js";
import { createAuthApi } from "./domain/auth.js";
import type { NodesApi } from "./domain/nodes.js";
import type { SlotsApi } from "./domain/slots.js";
import type { ConfigApi } from "./domain/config.js";
import type { EventsApi } from "./domain/events.js";
import type { LinksApi } from "./domain/links.js";
import type { LifecycleApi } from "./domain/lifecycle.js";
import type { SeedApi } from "./domain/seed.js";
import type { PluginsApi } from "./domain/plugins.js";
import type { KindsApi } from "./domain/kinds.js";
import type { AuthApi } from "./domain/auth.js";
import { REST_API_VERSION } from "./version.js";

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
}

/**
 * AgentClient — the public facade.
 *
 * Construct with `await AgentClient.connect(opts)`. The constructor is
 * private; `connect` performs the capability handshake before
 * returning.
 *
 * Dependency direction: client → domain → transport → schemas.
 * domain/* never imports fetch directly; transport/* never imports domain.
 */
export class AgentClient {
  readonly nodes: NodesApi;
  readonly slots: SlotsApi;
  readonly config: ConfigApi;
  readonly events: EventsApi;
  readonly links: LinksApi;
  readonly lifecycle: LifecycleApi;
  readonly seed: SeedApi;
  readonly plugins: PluginsApi;
  readonly kinds: KindsApi;
  readonly auth: AuthApi;

  private constructor(http: HttpClient, opts: AgentClientOptions) {
    this.nodes = createNodesApi(http, REST_API_VERSION);
    this.slots = createSlotsApi(http, REST_API_VERSION);
    this.config = createConfigApi(http, REST_API_VERSION);
    this.events = createEventsApi(opts.baseUrl, REST_API_VERSION, opts.token);
    this.links = createLinksApi(http, REST_API_VERSION);
    this.lifecycle = createLifecycleApi(http, REST_API_VERSION);
    this.seed = createSeedApi(http, REST_API_VERSION);
    this.plugins = createPluginsApi(http, REST_API_VERSION);
    this.kinds = createKindsApi(http, REST_API_VERSION);
    this.auth = createAuthApi(http, REST_API_VERSION);
  }

  /**
   * Create a connected AgentClient.
   * Fetches GET /api/v1/capabilities and asserts all required caps are
   * met before returning — throws CapabilityMismatchError on failure.
   */
  static async connect(opts: AgentClientOptions): Promise<AgentClient> {
    const httpOpts = {
      baseUrl: opts.baseUrl,
      ...(opts.token !== undefined && { token: opts.token }),
      ...(opts.timeoutMs !== undefined && { timeoutMs: opts.timeoutMs }),
    };
    const http = new HttpClient(httpOpts);
    const client = new AgentClient(http, opts);

    if (!opts.skipCapabilityCheck) {
      const capsApi = createCapabilitiesApi(http);
      const manifest = await capsApi.getManifest();
      capsApi.assertRequirements(manifest);
    }

    return client;
  }
}
