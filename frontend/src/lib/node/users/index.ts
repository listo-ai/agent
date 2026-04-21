// Logic (hooks) — import these to wire up state
export { useUsersList } from "./useUsersList";
export type { UsersListState, UsersListParams, UserNode, GrantRoleState } from "./types";
export { useGrantRole } from "./useGrantRole";

// UI (components) — import these to render; they carry zero business logic
export { UserCard } from "./UserCard";
export type { UserCardProps } from "./UserCard";
export { UsersTable } from "./UsersTable";
export type { UsersTableProps } from "./UsersTable";
export { GrantRoleDialog } from "./GrantRoleDialog";
export type { GrantRoleDialogProps } from "./GrantRoleDialog";
