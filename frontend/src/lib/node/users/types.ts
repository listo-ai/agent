import type { Tags } from "../tags";

// ---------------------------------------------------------------------------
// Domain types (mirror Rust DTOs — no Zod at this layer)
// ---------------------------------------------------------------------------

export interface UserNode {
  id: string;
  path: string;
  /** `display_name` slot — null when the node has never been named. */
  displayName: string | null;
  /** `email` slot — null for service accounts without an address. */
  email: string | null;
  /** `enabled` slot — defaults to true. */
  enabled: boolean;
  tags: Tags;
}

// ---------------------------------------------------------------------------
// List / query state
// ---------------------------------------------------------------------------

export type UsersListStatus = "loading" | "error" | "ready";

export interface UsersListParams {
  /** RSQL filter string, e.g. `tags.labels=contains=ops` */
  filter?: string;
  sort?: string;
  page?: number;
  size?: number;
}

export interface UsersListState {
  status: UsersListStatus;
  users: UserNode[];
  errorDetail: string | null;
  /** Reload the list from the server. */
  refetch: () => void;
}

// ---------------------------------------------------------------------------
// Grant-role mutation state
// ---------------------------------------------------------------------------

export type GrantRoleStatus = "idle" | "pending" | "ok" | "error";

export interface GrantRoleState {
  status: GrantRoleStatus;
  errorDetail: string | null;
  /** Submit the grant; resolves after server 202. */
  grant: (userId: string, role: string, bulkActionId: string) => Promise<void>;
  /** Reset back to idle (e.g. after showing success toast). */
  reset: () => void;
}
