import { Outlet } from "react-router-dom";
import { Topbar } from "./Topbar";
import { Sidebar } from "./Sidebar";
import { useUiStore } from "@/store/ui";
import { cn } from "@/lib/utils";

// Shell — outermost layout frame.
// Topbar spans full width; below it: sidebar (left) + main workbench area.

export function Shell() {
  const sidebarCollapsed = useUiStore((s) => s.sidebarCollapsed);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <Topbar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main
          className={cn(
            "flex-1 overflow-auto transition-all duration-200",
            sidebarCollapsed ? "ml-0" : "",
          )}
        >
          <Outlet />
        </main>
      </div>
    </div>
  );
}
