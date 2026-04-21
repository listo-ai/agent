import type { RequestTransport } from "../transport/request.js";
import {
  FlowDtoSchema,
  FlowMutationResultSchema,
  FlowRevisionDtoSchema,
  FlowRevisionListResponseSchema,
  FlowListResponseSchema,
} from "../schemas/flow.js";
import type {
  FlowDto,
  FlowMutationResult,
  FlowRevisionDto,
  FlowRevisionListResponse,
  FlowListResponse,
} from "../schemas/flow.js";

/**
 * Flows API — undo/redo/revert and revision history for flow documents.
 *
 * Phase 1 is fully shipped on the backend.  All endpoints live under
 * `/api/v1/flows`.
 *
 * Design reference: `docs/design/UNDO-REDO.md`.
 */
export interface FlowsApi {
  // ---- Reads ----

  /** List all flows, paged. */
  list(opts?: { limit?: number; offset?: number }): Promise<FlowListResponse>;

  /** Get a single flow by id. */
  get(id: string): Promise<FlowDto>;

  /**
   * List revisions for a flow, newest first.
   * Use `limit` + `offset` for pagination.
   */
  listRevisions(
    id: string,
    opts?: { limit?: number; offset?: number },
  ): Promise<FlowRevisionListResponse>;

  /**
   * Materialise the flow document as it was at a specific revision.
   * Returns the raw document value (same shape as `FlowDto.document`).
   */
  documentAt(id: string, revId: string): Promise<unknown>;

  // ---- Mutations ----

  /**
   * Create a new flow.
   *
   * @param document  Initial document payload.  May be an empty object `{}`.
   * @param author    Who is creating the flow (user id or `"system"`).
   */
  create(opts: {
    name: string;
    document: unknown;
    author?: string;
  }): Promise<FlowDto>;

  /**
   * Apply an edit to a flow document, writing a new revision.
   *
   * @param expectedHead  Optional OCC guard: the current head revision id.
   *                      Omit to skip the guard (internal / system edits).
   *                      Returns `409 Conflict` on mismatch.
   */
  edit(opts: {
    id: string;
    document: unknown;
    summary?: string;
    author?: string;
    expectedHead?: string;
  }): Promise<FlowMutationResult>;

  /**
   * Undo the last flow edit.
   * Appends a new `"undo"` revision — no destructive mutation to history.
   *
   * Returns `422 Unprocessable Entity` when already at the start of history.
   */
  undo(opts: {
    id: string;
    author?: string;
    expectedHead?: string;
  }): Promise<FlowMutationResult>;

  /**
   * Redo the next flow edit (reverse of the most recent undo).
   * Returns `422` when there is nothing to redo.
   *
   * @param expectedTarget  Optional: the revision id the client expects to
   *                        redo to (two-tab stale-cursor guard).
   *                        Omit for the simple one-tab case.
   */
  redo(opts: {
    id: string;
    author?: string;
    expectedHead?: string;
    expectedTarget?: string;
  }): Promise<FlowMutationResult>;

  /**
   * Revert the flow to an arbitrary previous revision.
   * Appends a new `"revert"` revision; does not delete history.
   */
  revert(opts: {
    id: string;
    targetRevId: string;
    author?: string;
    expectedHead?: string;
  }): Promise<FlowMutationResult>;

  /** Hard-delete a flow. */
  delete(id: string, expectedHead?: string): Promise<void>;
}

export function createFlowsApi(
  http: RequestTransport,
  apiVersion: number,
): FlowsApi {
  const base = `/api/v${apiVersion}/flows`;

  function buildListQs(opts?: { limit?: number; offset?: number }): string {
    const qs = new URLSearchParams();
    if (opts?.limit !== undefined) qs.set("limit", String(opts.limit));
    if (opts?.offset !== undefined) qs.set("offset", String(opts.offset));
    const s = qs.toString();
    return s ? `?${s}` : "";
  }

  return {
    async list(opts) {
      const raw = await http.get<unknown>(`${base}${buildListQs(opts)}`);
      return FlowListResponseSchema.parse(raw);
    },

    async get(id) {
      const raw = await http.get<unknown>(`${base}/${encodeURIComponent(id)}`);
      return FlowDtoSchema.parse(raw);
    },

    async listRevisions(id, opts) {
      const raw = await http.get<unknown>(
        `${base}/${encodeURIComponent(id)}/revisions${buildListQs(opts)}`,
      );
      return FlowRevisionListResponseSchema.parse(raw);
    },

    async documentAt(id, revId) {
      return http.get<unknown>(
        `${base}/${encodeURIComponent(id)}/revisions/${encodeURIComponent(revId)}`,
      );
    },

    async create({ name, document, author }) {
      const raw = await http.post<unknown>(base, {
        name,
        document,
        ...(author !== undefined && { author }),
      });
      return FlowDtoSchema.parse(raw);
    },

    async edit({ id, document, summary, author, expectedHead }) {
      const raw = await http.post<unknown>(
        `${base}/${encodeURIComponent(id)}/edit`,
        {
          document,
          ...(summary !== undefined && { summary }),
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
        },
      );
      return FlowMutationResultSchema.parse(raw);
    },

    async undo({ id, author, expectedHead }) {
      const raw = await http.post<unknown>(
        `${base}/${encodeURIComponent(id)}/undo`,
        {
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
        },
      );
      return FlowMutationResultSchema.parse(raw);
    },

    async redo({ id, author, expectedHead, expectedTarget }) {
      const raw = await http.post<unknown>(
        `${base}/${encodeURIComponent(id)}/redo`,
        {
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
          ...(expectedTarget !== undefined && { expected_target: expectedTarget }),
        },
      );
      return FlowMutationResultSchema.parse(raw);
    },

    async revert({ id, targetRevId, author, expectedHead }) {
      const raw = await http.post<unknown>(
        `${base}/${encodeURIComponent(id)}/revert`,
        {
          target_rev_id: targetRevId,
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
        },
      );
      return FlowMutationResultSchema.parse(raw);
    },

    async delete(id, expectedHead) {
      const qs = expectedHead
        ? `?expected_head=${encodeURIComponent(expectedHead)}`
        : "";
      await http.delete(`${base}/${encodeURIComponent(id)}${qs}`);
    },
  };
}

// Re-export types consumers may need without importing from schemas directly.
export type { FlowDto, FlowMutationResult, FlowRevisionDto, FlowRevisionListResponse, FlowListResponse };
