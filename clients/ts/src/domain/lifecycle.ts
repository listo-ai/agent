import type { HttpClient } from "../transport/http.js";
import { LifecycleSchema } from "../schemas/events.js";
import type { Lifecycle } from "../schemas/events.js";

/**
 * `POST /api/v1/lifecycle` `{path, to}` — transitions a node's lifecycle
 * through the legal-transition table enforced by `GraphStore::transition`.
 * Mirrors the Rust `Lifecycle` enum's snake_case serialisation.
 */
export interface LifecycleApi {
  transition(path: string, to: Lifecycle): Promise<Lifecycle>;
}

export function createLifecycleApi(http: HttpClient, apiVersion: number): LifecycleApi {
  const base = `/api/v${apiVersion}/lifecycle`;
  return {
    async transition(path: string, to: Lifecycle): Promise<Lifecycle> {
      const raw = await http.post<{ path: string; to: unknown }>(base, { path, to });
      return LifecycleSchema.parse(raw.to);
    },
  };
}
