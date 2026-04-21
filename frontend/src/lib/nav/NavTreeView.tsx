/**
 * NavTreeView — pure presentational sidebar tree for `ui.nav` nodes.
 *
 * Zero business logic. Drive it with data from `useNavTree`.
 *
 * Each leaf with a `path` navigates to `/ui/<path>` (the SDUI route).
 * Folder-only nodes (no path) collapse/expand their children.
 */
import { useState } from "react";
import { NavLink } from "react-router-dom";
import { ChevronRight, ChevronDown, AlertCircle, RefreshCw, LayoutGrid } from "lucide-react";
import { Skeleton } from "@listo/ui-kit";
import { Button } from "@listo/ui-kit";
import {
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
} from "@listo/ui-kit";
import type { NavNode, NavTreeState } from "./types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface NavTreeViewProps {
  state: NavTreeState;
}

// ---------------------------------------------------------------------------
// Recursive tree item
// ---------------------------------------------------------------------------

interface NavItemProps {
  node: NavNode;
  depth: number;
}

function NavItem({ node, depth }: NavItemProps) {
  const [open, setOpen] = useState(true);
  const label = node.title ?? node.id;
  const hasChildren = node.children.length > 0;
  const isLeaf = node.path !== null;

  if (depth === 0) {
    // Top-level: SidebarMenuItem + optional sub-menu
    return (
      <SidebarMenuItem>
        {isLeaf ? (
          <SidebarMenuButton asChild tooltip={label}>
            <NavLink to={`/ui/${encodeURIComponent(node.path!)}`}>
              <LayoutGrid className="h-4 w-4" aria-hidden />
              <span>{label}</span>
            </NavLink>
          </SidebarMenuButton>
        ) : (
          <SidebarMenuButton
            tooltip={label}
            onClick={() => setOpen((o) => !o)}
          >
            <LayoutGrid className="h-4 w-4" aria-hidden />
            <span>{label}</span>
            {hasChildren && (
              open
                ? <ChevronDown size={12} className="ml-auto" />
                : <ChevronRight size={12} className="ml-auto" />
            )}
          </SidebarMenuButton>
        )}

        {hasChildren && open && (
          <SidebarMenuSub>
            {node.children.map((child) => (
              <NavItem key={child.id} node={child} depth={depth + 1} />
            ))}
          </SidebarMenuSub>
        )}
      </SidebarMenuItem>
    );
  }

  // Nested: SidebarMenuSubItem
  return (
    <SidebarMenuSubItem>
      {isLeaf ? (
        <SidebarMenuSubButton asChild>
          <NavLink to={`/ui/${encodeURIComponent(node.path!)}`}>
            <span>{label}</span>
          </NavLink>
        </SidebarMenuSubButton>
      ) : (
        <>
          <SidebarMenuSubButton onClick={() => setOpen((o) => !o)}>
            <span>{label}</span>
            {hasChildren && (
              open
                ? <ChevronDown size={10} className="ml-auto" />
                : <ChevronRight size={10} className="ml-auto" />
            )}
          </SidebarMenuSubButton>
          {hasChildren && open && (
            <SidebarMenuSub>
              {node.children.map((child) => (
                <NavItem key={child.id} node={child} depth={depth + 1} />
              ))}
            </SidebarMenuSub>
          )}
        </>
      )}
    </SidebarMenuSubItem>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function NavTreeView({ state }: NavTreeViewProps) {
  if (state.status === "unconfigured") {
    return (
      <p className="px-2 py-3 text-xs italic text-muted-foreground">
        No navigation root configured.
      </p>
    );
  }

  if (state.status === "loading") {
    return (
      <div className="space-y-1.5 px-2 py-2">
        {Array.from({ length: 4 }).map((_, i) => (
          // eslint-disable-next-line react/no-array-index-key
          <Skeleton key={i} className="h-7 w-full rounded-md" />
        ))}
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div className="flex flex-col gap-2 px-2 py-2">
        <div className="flex items-center gap-1.5 text-xs text-destructive">
          <AlertCircle className="h-3.5 w-3.5 shrink-0" />
          <span className="truncate">{state.errorDetail}</span>
        </div>
        <Button variant="ghost" size="sm" className="h-6 w-full text-xs" onClick={() => state.refetch()}>
          <RefreshCw className="mr-1.5 h-3 w-3" />
          Retry
        </Button>
      </div>
    );
  }

  if (!state.root) {
    return (
      <p className="px-2 py-3 text-xs italic text-muted-foreground">No navigation entries.</p>
    );
  }

  return (
    <SidebarMenu>
      {/* Render root's children as top-level items (root itself is a container) */}
      {state.root.children.length > 0
        ? state.root.children.map((node) => (
            <NavItem key={node.id} node={node} depth={0} />
          ))
        : (
          <NavItem key={state.root.id} node={state.root} depth={0} />
        )}
    </SidebarMenu>
  );
}
