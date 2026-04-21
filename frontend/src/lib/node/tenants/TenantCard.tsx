/**
 * TenantCard — pure presentational card for a `sys.auth.tenant` node.
 *
 * Zero business logic. Drive it with data from the graph or construct
 * a `TenantNode` object manually for storybook / tests.
 */
import { Building2, Users, Tag } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import type { TenantNode } from "./types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface TenantCardProps {
  tenant: TenantNode;
  className?: string | undefined;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function TenantCard({ tenant, className }: TenantCardProps) {
  const { labels, kv } = tenant.tags;
  const kvEntries = Object.entries(kv);
  const hasTags = labels.length > 0 || kvEntries.length > 0;

  return (
    <Card className={cn("w-full", className)}>
      <CardHeader className="flex flex-row items-center gap-3 pb-2">
        {/* Icon */}
        <span
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground"
          aria-hidden
        >
          <Building2 className="h-5 w-5" />
        </span>

        {/* Identity */}
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium leading-none">
            {tenant.name ?? <span className="italic text-muted-foreground">unnamed tenant</span>}
          </p>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{tenant.path}</p>
        </div>

        {/* Member count */}
        <Badge variant="secondary" className="flex shrink-0 items-center gap-1 text-[11px]">
          <Users className="h-3 w-3" aria-hidden />
          {tenant.memberCount}
        </Badge>
      </CardHeader>

      {hasTags && (
        <CardContent className="pt-0">
          <div className="flex flex-wrap gap-1" aria-label="tenant tags">
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
        </CardContent>
      )}
    </Card>
  );
}
