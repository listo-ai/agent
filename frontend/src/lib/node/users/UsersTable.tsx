/**
 * UsersTable — pure presentational table for the `sys.auth.user` list.
 *
 * Zero business logic. Receives `UsersListState` from `useUsersList`.
 * Pair with `GrantRoleDialog` (driven by `useGrantRole`) to add actions.
 */
import { RefreshCw, AlertCircle, Tag } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { UsersListState, UserNode } from "./types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface UsersTableProps {
  state: UsersListState;
  /** Called when user clicks the row's "Grant Role" button. Omit to hide. */
  onGrantRole?: (userId: string) => void;
  className?: string | undefined;
}

// ---------------------------------------------------------------------------
// Internal sub-components (no hooks in any of these)
// ---------------------------------------------------------------------------

function TagCell({ user }: { user: UserNode }) {
  const { labels, kv } = user.tags;
  const kvEntries = Object.entries(kv);
  if (labels.length === 0 && kvEntries.length === 0) {
    return <span className="text-xs text-muted-foreground">—</span>;
  }
  return (
    <div className="flex flex-wrap gap-1">
      {labels.map((l) => (
        <Badge key={l} variant="outline" className="flex items-center gap-1 text-[11px]">
          <Tag className="h-2.5 w-2.5" aria-hidden />
          {l}
        </Badge>
      ))}
      {kvEntries.map(([k, v]) => (
        <Badge key={k} variant="outline" className="font-mono text-[11px]">
          {k}:{v}
        </Badge>
      ))}
    </div>
  );
}

function LoadingRows() {
  return (
    <>
      {Array.from({ length: 4 }).map((_, i) => (
        // eslint-disable-next-line react/no-array-index-key
        <TableRow key={i}>
          {Array.from({ length: 5 }).map((__, j) => (
            // eslint-disable-next-line react/no-array-index-key
            <TableCell key={j}>
              <Skeleton className="h-4 w-full" />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function UsersTable({ state, onGrantRole, className }: UsersTableProps) {
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {/* Error banner */}
      {state.status === "error" && (
        <div className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span>{state.errorDetail}</span>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto h-6 px-2"
            onClick={() => state.refetch()}
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}

      <div className="rounded-md border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-[200px]">Name</TableHead>
              <TableHead>Email</TableHead>
              <TableHead className="w-[90px] text-center">Status</TableHead>
              <TableHead>Tags</TableHead>
              {onGrantRole && <TableHead className="w-[110px]" />}
            </TableRow>
          </TableHeader>

          <TableBody>
            {state.status === "loading" && <LoadingRows />}

            {state.status === "ready" && state.users.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={onGrantRole ? 5 : 4}
                  className="py-8 text-center text-sm text-muted-foreground"
                >
                  No users found.
                </TableCell>
              </TableRow>
            )}

            {state.status === "ready" &&
              state.users.map((user) => (
                <TableRow key={user.id}>
                  <TableCell className="font-medium">
                    <span className="block truncate max-w-[180px]">
                      {user.displayName ?? (
                        <span className="italic text-muted-foreground">unnamed</span>
                      )}
                    </span>
                    <span className="mt-0.5 block truncate text-xs text-muted-foreground max-w-[180px]">
                      {user.path}
                    </span>
                  </TableCell>

                  <TableCell className="text-sm">
                    {user.email ?? <span className="text-muted-foreground">—</span>}
                  </TableCell>

                  <TableCell className="text-center">
                    <Badge
                      variant={user.enabled ? "default" : "secondary"}
                      className="text-[10px]"
                    >
                      {user.enabled ? "enabled" : "disabled"}
                    </Badge>
                  </TableCell>

                  <TableCell>
                    <TagCell user={user} />
                  </TableCell>

                  {onGrantRole && (
                    <TableCell>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => onGrantRole(user.id)}
                        className="h-7 text-xs"
                      >
                        Grant Role
                      </Button>
                    </TableCell>
                  )}
                </TableRow>
              ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
