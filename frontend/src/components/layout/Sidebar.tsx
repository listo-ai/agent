import { useEffect, useMemo, useState } from "react";
import { NavLink, useNavigate, useMatch } from "react-router-dom";
import { useStore, create } from "zustand";
import {
  GitBranch,
  FileText,
  Puzzle,
  Settings,
  ChevronRight,
  ChevronDown,
  Loader2,
  Box,
} from "lucide-react";
import type { Link, NodeSnapshot } from "@listo/agent-client";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { isFlowNode } from "@/pages/flows/flow-model";
import { buildNodeContextItems, buildCopyItems, NodeContextMenu } from "@/components/node-context-menu";
import { useRemoveNode } from "@/lib/node";
import { AddChildNodeDialog } from "@/components/AddChildNodeDialog";
import { cn } from "@/lib/utils";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  SidebarRail,
} from "@listo/ui-kit";
import { NavTreeView, NavModeToggle, useNavTree, useNavMode } from "@/lib/nav";
import {
  RemoteAgentsSection,
  ScopeIndicator,
  useRemoteAgents,
} from "@/lib/fleet";

// ---------------------------------------------------------------------------
// Static nav items (admin mode)
// ---------------------------------------------------------------------------

interface NavItem { label: string; to: string; icon: React.ElementType; }

const BOTTOM_NAV: NavItem[] = [
  { label: "Pages",    to: "/pages",    icon: FileText },
  { label: "Blocks",   to: "/blocks",   icon: Puzzle },
  { label: "Settings", to: "/settings", icon: Settings },
];

// ---------------------------------------------------------------------------
// Admin sidebar content (flows tree + pages/blocks/settings)
// ---------------------------------------------------------------------------

function AdminSidebarContent() {
  const remoteAgents = useRemoteAgents();

  return (
    <SidebarContent>
      <ScopeIndicator />

      <SidebarGroup>
        <SidebarGroupLabel>Flows</SidebarGroupLabel>
        <SidebarGroupContent>
          <FlowsTree />
        </SidebarGroupContent>
      </SidebarGroup>

      {remoteAgents.length > 0 && (
        <SidebarGroup>
          <SidebarGroupLabel>Remote Agents</SidebarGroupLabel>
          <SidebarGroupContent>
            <RemoteAgentsSection agents={remoteAgents} />
          </SidebarGroupContent>
        </SidebarGroup>
      )}

      <SidebarGroup>
        <SidebarGroupLabel>Navigation</SidebarGroupLabel>
        <SidebarGroupContent>
          <SidebarMenu>
            {BOTTOM_NAV.map(({ label, to, icon: Icon }) => (
              <SidebarMenuItem key={to}>
                <SidebarMenuButton asChild tooltip={label}>
                  <NavLink to={to}>
                    <Icon />
                    <span>{label}</span>
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>
    </SidebarContent>
  );
}

// ---------------------------------------------------------------------------
// User sidebar content (ui.nav tree)
// ---------------------------------------------------------------------------

function UserSidebarContent() {
  const { userNavRootId } = useNavMode();
  const navState = useNavTree(userNavRootId ?? undefined);

  return (
    <SidebarContent>
      <ScopeIndicator />
      <SidebarGroup>
        <SidebarGroupLabel>Navigation</SidebarGroupLabel>
        <SidebarGroupContent>
          <NavTreeView state={navState} />
        </SidebarGroupContent>
      </SidebarGroup>
    </SidebarContent>
  );
}

// ---------------------------------------------------------------------------
// AppSidebar — selects admin or user content; always shows the toggle
// ---------------------------------------------------------------------------

export function AppSidebar() {
  const { navMode, setNavMode } = useNavMode();

  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <NavLink to="/">
                <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground group-data-[collapsible=icon]:size-6 group-data-[collapsible=icon]:rounded-md">
                  <GitBranch size={16} />
                </div>
                <span className="font-semibold tracking-tight">Studio</span>
              </NavLink>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      {navMode === "admin" ? <AdminSidebarContent /> : <UserSidebarContent />}

      <SidebarFooter>
        <NavModeToggle
          mode={navMode}
          onToggle={() => setNavMode(navMode === "admin" ? "user" : "admin")}
        />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  );
}

// ---------------------------------------------------------------------------
// Context menu state
// ---------------------------------------------------------------------------

interface CtxMenu {
  x: number;
  y: number;
  node: NodeSnapshot;
  flowPath: string;
}

// ---------------------------------------------------------------------------
// Flows tree
// ---------------------------------------------------------------------------

const emptyStore = create(() => ({
  nodes: new Map<string, NodeSnapshot>(),
  links: new Map<string, Link>(),
  expanded: new Set<string>(),
  loadingPaths: new Set<string>(),
}));

function FlowsTree() {
  const navigate = useNavigate();
  const graphStore = useGraphStoreOptional();
  const active = graphStore ?? emptyStore;

  const nodeMap      = useStore(active, (s) => s.nodes);
  const expanded     = useStore(active, (s) => s.expanded);
  const loadingPaths = useStore(active, (s) => s.loadingPaths);

  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);
  const [addChildPath, setAddChildPath] = useState<string | null>(null);
  const closeCtx = () => setCtxMenu(null);
  const removeNode = useRemoveNode();

  function handleContextMenu(e: React.MouseEvent, node: NodeSnapshot, flowPath: string) {
    e.preventDefault();
    e.stopPropagation();
    setCtxMenu({ x: e.clientX, y: e.clientY, node, flowPath });
  }

  const { flows, childrenOf } = useMemo(() => {
    const flows: NodeSnapshot[] = [];
    const childrenOf = new Map<string, NodeSnapshot[]>();
    for (const n of nodeMap.values()) {
      if (isFlowNode(n)) flows.push(n);
      const parent = n.parent_path ?? "/";
      if (!childrenOf.has(parent)) childrenOf.set(parent, []);
      childrenOf.get(parent)!.push(n);
    }
    flows.sort((a, b) => a.path.localeCompare(b.path));
    for (const kids of childrenOf.values()) {
      kids.sort((a, b) => a.path.localeCompare(b.path));
    }
    return { flows, childrenOf };
  }, [nodeMap]);

  useEffect(() => {
    if (!graphStore) return;
    graphStore.getState().expand("/");
  }, [graphStore]);

  function toggle(flowPath: string) {
    if (!graphStore) return;
    const s = graphStore.getState();
    expanded.has(flowPath) ? s.collapse(flowPath) : s.expand(flowPath);
  }

  const rootLoading = loadingPaths.has("/") && flows.length === 0;

  return (
    <>
      {ctxMenu && (
        <NodeContextMenu
          x={ctxMenu.x}
          y={ctxMenu.y}
          nodeLabel={nodeName(ctxMenu.node.path)}
          onClose={closeCtx}
          items={[
            ...buildNodeContextItems({
              onOpen: () => { navigate(`/flows/edit${ctxMenu.node.path}`); closeCtx(); },
              onAddChild: () => { setAddChildPath(ctxMenu.node.path); closeCtx(); },
              onHistory: () => { navigate(`/flows/edit${ctxMenu.node.path}`); closeCtx(); },
              onSettings: () => { navigate(`/flows/edit${ctxMenu.node.path}`); closeCtx(); },
              onDelete: () => {
                closeCtx();
                removeNode.mutate(ctxMenu.node.path);
              },
            }),
            ...buildCopyItems({
              path: ctxMenu.node.path,
              kindId: ctxMenu.node.kind,
            }),
          ]}
        />
      )}
      {addChildPath && (
        <AddChildNodeDialog
          parentPath={addChildPath}
          onClose={() => setAddChildPath(null)}
          onCreated={() => setAddChildPath(null)}
        />
      )}

      <SidebarMenu>
        <SidebarMenuItem>
          <SidebarMenuButton asChild tooltip="Flows">
            <NavLink to="/flows" end>
              <GitBranch />
              <span>All flows</span>
              {rootLoading && <Loader2 size={12} className="ml-auto animate-spin text-muted-foreground" />}
            </NavLink>
          </SidebarMenuButton>
        </SidebarMenuItem>

        {flows.map((flow) => (
          <FlowTreeNode
            key={flow.path}
            node={flow}
            depth={0}
            flowPath={flow.path}
            childrenOf={childrenOf}
            expanded={expanded}
            loadingPaths={loadingPaths}
            onToggle={toggle}
            navigate={navigate}
            onContextMenu={handleContextMenu}
          />
        ))}
      </SidebarMenu>
    </>
  );
}

// ---------------------------------------------------------------------------
// Recursive tree node
// ---------------------------------------------------------------------------

interface FlowTreeNodeProps {
  node: NodeSnapshot;
  depth: number;
  flowPath: string;
  childrenOf: Map<string, NodeSnapshot[]>;
  expanded: Set<string>;
  loadingPaths: Set<string>;
  onToggle(path: string): void;
  navigate: ReturnType<typeof useNavigate>;
  onContextMenu(e: React.MouseEvent, node: NodeSnapshot, flowPath: string): void;
}

function FlowTreeNode({
  node, depth, flowPath, childrenOf, expanded, loadingPaths, onToggle, navigate, onContextMenu,
}: FlowTreeNodeProps) {
  const isOpen    = expanded.has(node.path);
  const isLoading = loadingPaths.has(node.path);
  const children  = childrenOf.get(node.path) ?? [];
  const name      = nodeName(node.path);
  const isFlow    = depth === 0;

  const match    = useMatch(`/flows/edit${node.path === flowPath ? flowPath : node.path}`);
  const subMatch = useMatch(`/flows/edit${node.path}/*`);
  const isActive = !!(match || subMatch);

  const Icon = isFlow ? GitBranch : Box;

  if (depth === 0) {
    return (
      <SidebarMenuItem>
        <SidebarMenuButton
          isActive={isActive}
          tooltip={node.path}
          onContextMenu={(e) => onContextMenu(e, node, flowPath)}
          onClick={() => {
            onToggle(node.path);
            navigate(`/flows/edit${flowPath}`);
          }}
        >
          <Icon />
          <span>{name}</span>
          {(node.has_children || isOpen) && (
            isLoading && !isOpen
              ? <Loader2 size={12} className="ml-auto animate-spin" />
              : isOpen
                ? <ChevronDown size={12} className="ml-auto" />
                : <ChevronRight size={12} className="ml-auto" />
          )}
        </SidebarMenuButton>

        {isOpen && (
          <SidebarMenuSub>
            {isLoading && children.length === 0 && (
              <SidebarMenuSubItem>
                <span className="flex items-center gap-1.5 px-2 py-0.5 text-xs text-muted-foreground">
                  <Loader2 size={11} className="animate-spin" />Loading…
                </span>
              </SidebarMenuSubItem>
            )}
            {children.map((child) => (
              <FlowTreeNode
                key={child.path}
                node={child}
                depth={depth + 1}
                flowPath={flowPath}
                childrenOf={childrenOf}
                expanded={expanded}
                loadingPaths={loadingPaths}
                onToggle={onToggle}
                navigate={navigate}
                onContextMenu={onContextMenu}
              />
            ))}
            {!isLoading && children.length === 0 && (
              <SidebarMenuSubItem>
                <span className="block px-2 py-0.5 text-xs italic text-muted-foreground/50">empty</span>
              </SidebarMenuSubItem>
            )}
          </SidebarMenuSub>
        )}
      </SidebarMenuItem>
    );
  }

  return (
    <SidebarMenuSubItem>
      <SidebarMenuSubButton
        isActive={isActive}
        onContextMenu={(e) => onContextMenu(e, node, flowPath)}
        onClick={() => navigate(`/flows/edit${flowPath}`)}
        className={cn("cursor-pointer", depth > 1 ? "pl-4" : undefined)}
      >
        <Icon className="size-3 shrink-0 opacity-50" />
        <span>{name}</span>
      </SidebarMenuSubButton>
    </SidebarMenuSubItem>
  );
}

// ---------------------------------------------------------------------------
// Util
// ---------------------------------------------------------------------------

function nodeName(path: string): string {
  if (!path || path === "/") return "/";
  const parts = path.replace(/\/$/, "").split("/");
  return parts[parts.length - 1] ?? path;
}
