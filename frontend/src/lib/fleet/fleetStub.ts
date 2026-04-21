/**
 * Stub `FleetRequestFn` — used until a real fleet WS/NATS connection lands.
 *
 * Design intent
 * -------------
 * The `fleetRequestFn` passed to `AgentClient.connect({ fleetRequestFn })`
 * is the only backend-specific seam in the whole client stack. Everything
 * above it (`FleetRequestTransport`, all domain modules) is backend-agnostic.
 *
 * This module exports the stub so it can be referenced in `ScopeProvider`
 * today and replaced by a real implementation (e.g. a Zenoh WS or NATS-WS
 * client) without touching any other file. The swap is:
 *
 *   1. Create `src/lib/fleet/zenohFleetRequest.ts` (or `natsFleetRequest.ts`)
 *      exporting a function with the same `FleetRequestFn` signature.
 *   2. Change the import in `ScopeProvider.tsx` from `fleetStub` to the
 *      new module.  Nothing else changes.
 *
 * The stub throws a typed error rather than silently no-oping so UI
 * components fail fast with a clear message instead of hanging on timeout.
 */

import type { FleetRequestFn } from "@sys/agent-client";

/**
 * Fleet request function stub.
 *
 * Replace this with a real WS/NATS implementation when the transport
 * backend lands. Signature must remain `FleetRequestFn`-compatible.
 *
 * @throws `FleetNotConnectedError` — always, until a real backend exists.
 */
export const fleetRequestStub: FleetRequestFn = async (
  subject: string,
  _body: unknown,
  _timeoutMs: number,
): Promise<unknown> => {
  throw new FleetNotConnectedError(subject);
};

/**
 * Thrown when a fleet request is attempted without a live connection.
 *
 * Callers can distinguish this from other errors to show a
 * "fleet transport not connected" message instead of a generic error banner.
 */
export class FleetNotConnectedError extends Error {
  readonly subject: string;

  constructor(subject: string) {
    super(
      `Fleet transport not connected — cannot reach subject "${subject}". ` +
        "Wire a real fleetRequestFn (Zenoh WS / NATS-WS) to enable remote agent access.",
    );
    this.name = "FleetNotConnectedError";
    this.subject = subject;
  }
}
