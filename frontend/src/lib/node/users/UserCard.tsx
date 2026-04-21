/**
 * UserCard — pure presentational card for a single `sys.auth.user` node.
 *
 * Zero business logic — all state comes in via props. Pair with
 * `useUsersList` (for list data) or use standalone in a detail panel.
 */
import { User, Mail, Tag } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { UserNode } from "./types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface UserCardProps {
  user: UserNode;
  /** Called when the user clicks "Grant Role". Omit to hide the button. */
  onGrantRole?: (userId: string) => void;
  className?: string | undefined;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function initials(name: string | null, email: string | null): string {
  const src = name ?? email ?? "?";
  return src
    .split(/[\s@.]/)
    .filter(Boolean)
    .slice(0, 2)
    .map((s) => s[0]?.toUpperCase() ?? "")
    .join("");
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function UserCard({ user, onGrantRole, className }: UserCardProps) {
  const initStr = initials(user.displayName, user.email);
  const { labels, kv } = user.tags;
  const kvEntries = Object.entries(kv);
  const hasAny = labels.length > 0 || kvEntries.length > 0;

  return (
    <Card className={cn("w-full", className)}>
      <CardHeader className="flex flex-row items-center gap-3 pb-2">
        {/* Avatar */}
        <span
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted text-sm font-semibold text-muted-foreground"
          aria-hidden
        >
          {initStr}
        </span>

        {/* Identity */}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium leading-none">
            {user.displayName ?? <span className="italic text-muted-foreground">unnamed</span>}
          </p>
          {user.email && (
            <p className="mt-0.5 flex items-center gap-1 truncate text-xs text-muted-foreground">
              <Mail className="h-3 w-3 shrink-0" aria-hidden />
              {user.email}
            </p>
          )}
        </div>

        {/* Enabled indicator */}
        <Badge
          variant={user.enabled ? "default" : "secondary"}
          className="shrink-0 text-[10px]"
        >
          {user.enabled ? "enabled" : "disabled"}
        </Badge>
      </CardHeader>

      <CardContent className="space-y-2 pt-0">
        {/* Tags */}
        {hasAny && (
          <div className="flex flex-wrap gap-1" aria-label="tags">
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
        )}

        {/* Actions */}
        {onGrantRole && (
          <div className="flex justify-end pt-1">
            <Button
              size="sm"
              variant="outline"
              onClick={() => onGrantRole(user.id)}
              className="flex items-center gap-1.5"
            >
              <User className="h-3.5 w-3.5" aria-hidden />
              Grant Role
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
