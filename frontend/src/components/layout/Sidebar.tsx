import { NavLink } from "react-router-dom";
import {
  GitBranch,
  LayoutDashboard,
  Puzzle,
  Settings,
  PanelLeftClose,
  PanelLeft,
} from "lucide-react";
import { useUiStore } from "@/store/ui";
import { cn } from "@/lib/utils";

interface NavItem {
  label: string;
  to: string;
  icon: React.ElementType;
}

const NAV_ITEMS: NavItem[] = [
  { label: "Flows",      to: "/flows",      icon: GitBranch },
  { label: "Dashboard",  to: "/dashboard",  icon: LayoutDashboard },
  { label: "Extensions", to: "/extensions", icon: Puzzle },
  { label: "Settings",   to: "/settings",   icon: Settings },
];

export function Sidebar() {
  const { sidebarCollapsed, toggleSidebar } = useUiStore();

  return (
    <aside
      style={{ width: sidebarCollapsed ? "48px" : "var(--sidebar-width)" }}
      className="flex h-full flex-col border-r border-border bg-card transition-all duration-200"
    >
      <nav className="flex flex-1 flex-col gap-1 p-2">
        {NAV_ITEMS.map(({ label, to, icon: Icon }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
                "hover:bg-accent hover:text-accent-foreground",
                isActive
                  ? "bg-accent text-accent-foreground font-medium"
                  : "text-muted-foreground",
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
        className="flex items-center justify-center p-2 text-muted-foreground hover:text-foreground"
        aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
      >
        {sidebarCollapsed ? <PanelLeft size={16} /> : <PanelLeftClose size={16} />}
      </button>
    </aside>
  );
}
