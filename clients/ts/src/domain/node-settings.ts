import type { RequestTransport } from "../transport/request.js";
import {
  NodeSettingRevisionListResponseSchema,
  NodeSettingsAtRevisionSchema,
  NodeSettingsMutationResultSchema,
} from "../schemas/node-settings.js";
import type {
  NodeSettingRevisionDto,
  NodeSettingRevisionListResponse,
  NodeSettingsAtRevision,
  NodeSettingsMutationResult,
} from "../schemas/node-settings.js";

/**
 * Per-node settings undo / redo / revert — Phase 2 of the undo/redo system.
 *
 * All endpoints live under `/api/v1/nodes/{id}/settings/`.
 *
 * **Phase 2 is pending on the backend** — these methods will return HTTP 404
 * until `crates/transport-rest` wires the handlers.  The client is shipped
 * now so the frontend can code against it and the backend can be dropped in
 * without a client change.
 *
 * Design reference: docs/design/UNDO-REDO.md § Phase 2
 */
export interface NodeSettingsApi {
  /**
   * List revision history for a single node's settings, newest first.
   *
   * @param nodeId  The node's ULID (not its path).
   */
  listRevisions(
    nodeId: string,
    opts?: { limit?: number; offset?: number },
  ): Promise<NodeSettingRevisionListResponse>;

  /**
   * Materialise the node's settings as they were at a specific revision.
   *
   * Returns `migration_status: "unavailable"` when the kind schema has
   * advanced past the revision and no migration chain exists — revert is
   * disabled in that case (see UNDO-REDO.md § Historical settings materialisation).
   */
  settingsAt(nodeId: string, revId: string): Promise<NodeSettingsAtRevision>;

  /**
   * Undo the last setting change for this node.
   * Returns `422 Unprocessable Entity` when already at the start of history.
   *
   * @param expectedHead  Optional OCC guard.  Omit to skip the check.
   */
  undo(opts: {
    nodeId: string;
    author?: string;
    expectedHead?: string;
  }): Promise<NodeSettingsMutationResult>;

  /**
   * Redo the next setting change (reverse of the most recent undo).
   * Returns `422` when there is nothing to redo.
   *
   * @param expectedTarget  Optional: the revision the client expects to redo to
   *                        (two-tab stale-cursor guard).
   */
  redo(opts: {
    nodeId: string;
    author?: string;
    expectedHead?: string;
    expectedTarget?: string;
  }): Promise<NodeSettingsMutationResult>;

  /**
   * Revert this node's settings to an arbitrary earlier revision.
   * Appends a new `"revert"` revision — does not delete history.
   *
   * Blocked when the target revision's `migration_status != "ok"`.
   */
  revert(opts: {
    nodeId: string;
    targetRevId: string;
    author?: string;
    expectedHead?: string;
  }): Promise<NodeSettingsMutationResult>;
}

export function createNodeSettingsApi(
  http: RequestTransport,
  apiVersion: number,
): NodeSettingsApi {
  const nodeBase = (nodeId: string) =>
    `/api/v${apiVersion}/nodes/${encodeURIComponent(nodeId)}/settings`;

  function buildListQs(opts?: { limit?: number; offset?: number }): string {
    const qs = new URLSearchParams();
    if (opts?.limit !== undefined) qs.set("limit", String(opts.limit));
    if (opts?.offset !== undefined) qs.set("offset", String(opts.offset));
    const s = qs.toString();
    return s ? `?${s}` : "";
  }

  return {
    async listRevisions(nodeId, opts) {
      const raw = await http.get<unknown>(
        `${nodeBase(nodeId)}/revisions${buildListQs(opts)}`,
      );
      return NodeSettingRevisionListResponseSchema.parse(raw);
    },

    async settingsAt(nodeId, revId) {
      const raw = await http.get<unknown>(
        `${nodeBase(nodeId)}/revisions/${encodeURIComponent(revId)}`,
      );
      return NodeSettingsAtRevisionSchema.parse(raw);
    },

    async undo({ nodeId, author, expectedHead }) {
      const raw = await http.post<unknown>(
        `${nodeBase(nodeId)}/undo`,
        {
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
        },
      );
      return NodeSettingsMutationResultSchema.parse(raw);
    },

    async redo({ nodeId, author, expectedHead, expectedTarget }) {
      const raw = await http.post<unknown>(
        `${nodeBase(nodeId)}/redo`,
        {
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
          ...(expectedTarget !== undefined && { expected_target: expectedTarget }),
        },
      );
      return NodeSettingsMutationResultSchema.parse(raw);
    },

    async revert({ nodeId, targetRevId, author, expectedHead }) {
      const raw = await http.post<unknown>(
        `${nodeBase(nodeId)}/revert`,
        {
          target_rev_id: targetRevId,
          ...(author !== undefined && { author }),
          ...(expectedHead !== undefined && { expected_head: expectedHead }),
        },
      );
      return NodeSettingsMutationResultSchema.parse(raw);
    },
  };
}

export type {
  NodeSettingRevisionDto,
  NodeSettingRevisionListResponse,
  NodeSettingsAtRevision,
  NodeSettingsMutationResult,
};
