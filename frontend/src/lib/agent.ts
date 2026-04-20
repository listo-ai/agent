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

// PUBLIC_AGENT_URL is injected at build time by rsbuild (source.define in
// rsbuild.config.ts reads process.env.PUBLIC_AGENT_URL). Direct property
// access is required — dynamic/cast access defeats static substitution.
export const AGENT_BASE_URL =
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore — rsbuild injects this at build time
  (import.meta.env.PUBLIC_AGENT_URL as string | undefined) ?? "http://localhost:8080";

export const agentPromise: Promise<AgentClient> = AgentClient.connect({
  baseUrl: AGENT_BASE_URL,
  skipCapabilityCheck: true,
});
