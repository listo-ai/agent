/**
 * Module-level singleton `AgentClient` instance used by React hooks.
 *
 * Exposed as a promise so callers consume it via `useQuery` (see
 * [`useAgent`](../hooks/useAgent.ts)) — that way the connect flow, once
 * the capability handshake goes live, automatically surfaces as a
 * Query error instead of a thrown top-level render.
 *
 * Base URL resolves from `PUBLIC_AGENT_URL` (Rsbuild convention),
 * defaulting to the agent's stock bind address.
 *
 * Capability check is intentionally skipped for now — studio doesn't
 * gate its UI on it yet; the `useCapabilities` hook surfaces the
 * manifest so the UI can display it.
 */
import { AgentClient } from "@sys/agent-client";

const env = (import.meta as unknown as { env: Record<string, string | undefined> }).env;

export const AGENT_BASE_URL = env["PUBLIC_AGENT_URL"] ?? "http://localhost:8080";

export const agentPromise: Promise<AgentClient> = AgentClient.connect({
  baseUrl: AGENT_BASE_URL,
  skipCapabilityCheck: true,
});
