import { useEffect, useMemo } from "react";
import { NavLink, useNavigate, useMatch } from "react-router-dom";
import { useStore, create } from "zustand";
import {
  GitBranch,
  LayoutDashboard,
  Package,
  Puzzle,
  Settings,
  PanelLeftClose,
  PanelLeft,
  ChevronRight,
  ChevronDown,
  Loader2,
  Box,
} from "lucide-react";
import type { Link, NodeSnapshot } from "@acme/agent-client";
import { useUiStore } from "@/store/ui";
import { useGraphStoreOptional } from "@/store/graph-hooks";
import { isFlowNode } from "@/pages/flows/flow-model";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Static nav items rendered below the flows tree
// ---------------------------------------------------------------------------

interface NavItem { label: string; to: string; icon: React.ElementType; }

const BOTTOM_NAV: NavItem[] = [
  { label: "Dashboard",  to: "/dashboard",  icon: LayoutDashboard },
  { label: "Extensions", to: "/extensions", icon: Puzzle },
  { label: "Plugins",    to: "/plugins",    icon: Package },
  { label: "Settings",   to: "/settings",   icon: Settings },
];

// ---------------------------------------------------------------------------
// Root sidebar
// ---------------------------------------------------------------------------

export function Sidebar() {
  const { sidebarCollapsed, toggleSidebar } = useUiStore();

  return (
    <aside
      style={{ width: sidebarCollapsed ? "48px" : "var(--sidebar-width)" }}
      className="flex h-full flex-col border-r border-border bg-card transition-all duration-200 overflow-hidden"
    >
      <nav className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
        <FlowsTree collapsed={sidebarCollapsed} />

        {!sidebarCollapsed && <div className="my-1 h-px bg-border" />}

        {BOTTOM_NAV.map(({ label, to, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
                "hover:bg-accent hover:text-accent-foreground",
                isActive ? "bg-accent text-accent-foreground font-medium" : "text-muted-foreground",
              )
            }
          >
            <Icon size={16} className="shrink-0" />
            {!sidebarCollapsed && <span>{label}</span>}
          </NavLink>
        ))}
      </nav>

      <button
        onClick={toggleSidebar}
        className="flex shrink-0 items-center justify-center p-2 text-muted-foreground hover:text-foreground"
        aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {sidebarCollapsed ? <PanelLeft size={16} /> : <PanelLeftClose size={16} />}
      </button>
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Flows tree
// ---------------------------------------------------------------------------

/** Stable empty store used while the GraphStore is still connecting. */
const emptyStore = create(() => ({
  nodes: new Map<string, NodeSnapshot>(),
  links: new Map<string, Link>(),
  expanded: new Set<string>(),
  loadingPaths: new Set<string>(),
}));

function FlowsTree({ collapsed }: { collapsed: boolean }) {
  const navigate = useNavigate();
  const graphStore = useGraphStoreOptional();
  const active = graphStore ?? emptyStore;

  const nodeMap      = useStore(active, (s) => s.nodes);
  const expanded     = useStore(active, (s) => s.expanded);
  const loadingPaths = useStore(active, (s) => s.loadingPaths);

  const { flows, childrenOf } = useMemo(() => {
    const flows: NodeSnapshot[] = [];
    // Build parent→children index for all cached nodes
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

  // Eagerly fetch root children so flow rows appear as soon as the store connects.
  useEffect(() => {
    if (!graphStore) return;
    graphStore.getState().expand("/");
  }, [graphStore]);

  function toggle(flowPath: string) {
    if (!graphStore) return;
    const s = graphStore.getState();
    expanded.has(flowPath) ? s.collapse(flowPath) : s.expand(flowPath);
  }

  // Collapsed sidebar: single icon that links to /flows
  if (collapsed) {
    return (
      <NavLink
        to="/flows"
        className={({ isActive }) =>
          cn(
            "flex items-center justify-center rounded-md p-1.5 transition-colors",
            "hover:bg-accent hover:text-accent-foreground",
            isActive ? "text-accent-foreground" : "text-muted-foreground",
          )
        }
      >
        <GitBranch size={16} />
      </NavLink>
    );
  }

  const rootLoading = loadingPaths.has("/") && flows.length === 0;

  return (
    <div>
      {/* "Flows" section header */}
      <NavLink
        to="/flows"
        end
        className={({ isActive }) =>
          cn(
            "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm font-medium transition-colors",
            "hover:bg-accent hover:text-accent-foreground",
            isActive ? "bg-accent text-accent-foreground" : "text-foreground",
          )
        }
      >
        <GitBranch size={16} className="shrink-0" />
        <span>Flows</span>
        {rootLoading && <Loader2 size={12} className="ml-auto animate-spin text-muted-foreground" />}
      </NavLink>

      {/* One row per flow, each recursively expandable */}
      {flows.map((flow) => (
        <TreeNode
          key={flow.path}
          node={flow}
          depth={0}
          flowPath={flow.path}
          childrenOf={childrenOf}
          expanded={expanded}
          loadingPaths={loadingPaths}
          onToggle={toggle}
          navigate={navigate}
        />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recursive tree node
// ---------------------------------------------------------------------------

interface TreeNodeProps {
  node: NodeSnapshot;
  depth: number;
  flowPath: string;                             // top-level flow path for navigation
  childrenOf: Map<string, NodeSnapshot[]>;
  expanded: Set<string>;
  loadingPaths: Set<string>;
  onToggle(path: string): void;
  navigate: ReturnType<typeof useNavigate>;
}

function TreeNode({ node, depth, flowPath, childrenOf, expanded, loadingPaths, onToggle, navigate }: TreeNodeProps) {
  const isOpen    = expanded.has(node.path);
  const isLoading = loadingPaths.has(node.path);
  const children  = childrenOf.get(node.path) ?? [];
  const name      = nodeName(node.path);
  const isFlow    = depth === 0;

  // Match any sub-path under this flow for active highlight
  const match = useMatch(`/flows/edit${node.path === flowPath ? flowPath : node.path}`);
  const subMatch = useMatch(`/flows/edit${node.path}/*`);
  const isActive = !!(match || subMatch);

  const Icon = isFlow ? GitBranch : Box;
  const iconSize = isFlow ? 13 : 11;
  const textClass = isFlow ? "text-sm" : "text-xs";

  return (
    <div>
      <div
        className={cn(
          "flex items-center gap-0.5 rounded-md transition-colors",
          "hover:bg-accent hover:text-accent-foreground",
          isActive
            ? "bg-accent/60 text-accent-foreground font-medium"
            : "text-muted-foreground",
          textClass,
        )}
        style={{ paddingLeft: depth > 0 ? `${depth * 8}px` : undefined }}
      >
        {/* chevron / dot */}
        <button
          type="button"
          onClick={(e) => { e.stopPropagation(); onToggle(node.path); }}
          className="flex h-6 w-5 shrink-0 items-center justify-center rounded hover:text-foreground"
          aria-label={isOpen ? "Collapse" : "Expand"}
        >
          {isLoading && !isOpen
            ? <Loader2 size={11} className="animate-spin" />
            : (node.has_children || isOpen)
              ? isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />
              : <span className="block h-1.5 w-1.5 rounded-full bg-border" />
          }
        </button>

        {/* label */}
        <button
          type="button"
          onClick={() => navigate(`/flows/edit${flowPath}`)}
          className="flex min-w-0 flex-1 items-center gap-1.5 py-0.5 pr-2 text-left"
          title={node.path}
        >
          <Icon size={iconSize} className="shrink-0 opacity-50" />
          <span className="truncate">{name}</span>
        </button>
      </div>

      {/* Children (recursive) */}
      {isOpen && (
        <div className="border-l border-border" style={{ marginLeft: `${(depth + 1) * 8 + 10}px` }}>
          {isLoading && children.length === 0 && (
            <span className="flex items-center gap-1.5 px-2 py-0.5 text-xs text-muted-foreground">
              <Loader2 size={11} className="animate-spin" />Loading…
            </span>
          )}
          {children.map((child) => (
            <TreeNode
              key={child.path}
              node={child}
              depth={depth + 1}
              flowPath={flowPath}
              childrenOf={childrenOf}
              expanded={expanded}
              loadingPaths={loadingPaths}
              onToggle={onToggle}
              navigate={navigate}
            />
          ))}
          {!isLoading && children.length === 0 && (
            <span className="block px-2 py-0.5 text-xs italic text-muted-foreground/50">
              empty
            </span>
          )}
        </div>
      )}
    </div>
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
