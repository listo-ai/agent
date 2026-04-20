import { Outlet } from "react-router-dom";
import { SiteHeader } from "./SiteHeader";
import { AppSidebar } from "./Sidebar";
import { SidebarProvider, SidebarInset } from "@/components/ui";
import { GlobalAiChat, useChatContextSync } from "@/features/global-ai-chat";

// Shell — outermost layout frame using shadcn sidebar-16 pattern.
// SidebarProvider owns collapse state (persisted in cookie).

export function Shell() {
  // Keep the global-chat context store aligned with the current route.
  useChatContextSync();
  return (
    <SidebarProvider className="h-full min-h-0">
      <AppSidebar />
      <SidebarInset>
        <SiteHeader />
        <div className="flex min-h-0 flex-1 flex-col">
          <Outlet />
        </div>
      </SidebarInset>
      <GlobalAiChat />
    </SidebarProvider>
  );
}

