import { HttpClient } from "./transport/http.js";
import { createNodesApi } from "./domain/nodes.js";
import { createSlotsApi } from "./domain/slots.js";
import { createConfigApi } from "./domain/config.js";
import { createEventsApi } from "./domain/events.js";
import { createCapabilitiesApi } from "./domain/capabilities.js";
import type { NodesApi } from "./domain/nodes.js";
import type { SlotsApi } from "./domain/slots.js";
import type { ConfigApi } from "./domain/config.js";
import type { EventsApi } from "./domain/events.js";
import { REST_API_VERSION } from "./version.js";

export interface AgentClientOptions {
  /** Base URL of the agent, e.g. "http://localhost:4000". */
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
 * Construct with `await AgentClient.connect(opts)`.  The constructor is
 * private; `connect` performs the capability handshake before returning.
 *
 * Dependency direction: client → domain → transport → schemas.
 * domain/* never imports fetch directly; transport/* never imports domain.
 */
export class AgentClient {
  readonly nodes: NodesApi;
  readonly slots: SlotsApi;
  readonly config: ConfigApi;
  readonly events: EventsApi;

  private constructor(
    private readonly http: HttpClient,
    private readonly opts: AgentClientOptions,
  ) {
    this.nodes  = createNodesApi(http, REST_API_VERSION);
    this.slots  = createSlotsApi(http, REST_API_VERSION);
    this.config = createConfigApi(http, REST_API_VERSION);
    this.events = createEventsApi(opts.baseUrl, REST_API_VERSION, opts.token);
  }

  /**
   * Create a connected AgentClient.
   * Fetches GET /api/v1/capabilities and asserts all required caps are met
   * before returning — throws CapabilityMismatchError on failure.
   */
  static async connect(opts: AgentClientOptions): Promise<AgentClient> {
    const httpOpts = {
      baseUrl: opts.baseUrl,
      ...(opts.token      !== undefined && { token:     opts.token }),
      ...(opts.timeoutMs  !== undefined && { timeoutMs: opts.timeoutMs }),
    };
    const http = new HttpClient(httpOpts);
    const client = new AgentClient(http, opts);

    if (!opts.skipCapabilityCheck) {
      const capsApi  = createCapabilitiesApi(http);
      const manifest = await capsApi.getManifest();
      capsApi.assertRequirements(manifest);
    }

    return client;
  }
}
