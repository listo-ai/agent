import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { Shell } from "@/components/layout/Shell";
import { FlowsListPage } from "@/pages/flows/FlowsListPage";
import { FlowsPage } from "@/pages/flows/FlowsPage";
import { PagesListPage } from "@/pages/pages/PagesListPage";
import { PageBuilderPage } from "@/features/page-builder/PageBuilderPage";
import { BlocksPage } from "@/pages/blocks/BlocksPage";
import { SettingsPage } from "@/pages/settings/SettingsPage";
import { SduiPage } from "@/sdui/SduiPage";
import { SduiRenderPage } from "@/sdui/SduiRenderPage";
import { ScopeProvider } from "@/lib/fleet";

// ---------------------------------------------------------------------------
// Shell children — shared between the local route and the /scope/... route
// so adding a new page only requires one edit.
// ---------------------------------------------------------------------------

const shellChildren = [
  { index: true, element: <FlowsListPage /> },
  { path: "flows", element: <FlowsListPage /> },
  // `*` splat captures the flow's graph path (which contains `/`).
  // Example: /flows/edit/flow-1 → flow path "/flow-1".
  { path: "flows/edit/*", element: <FlowsPage /> },
  // Short-form deep-link: /flows/flow-1 → canvas for "/flow-1".
  // Matched after flows/edit/* so the edit prefix still wins.
  { path: "flows/*", element: <FlowsPage /> },
  { path: "pages", element: <PagesListPage /> },
  { path: "pages/:id/edit", element: <PageBuilderPage /> },
  // SDUI renderer — any ui.page node can be navigated to directly.
  { path: "ui/:pageRef", element: <SduiPage /> },
  // SDUI render — pick the target kind's default view (S5).
  { path: "render/:targetId", element: <SduiRenderPage /> },
  { path: "blocks", element: <BlocksPage /> },
  { path: "settings", element: <SettingsPage /> },
];

// ---------------------------------------------------------------------------
// ScopedShell — Shell wrapped in ScopeProvider for /scope/... routes.
//
// ScopeProvider reads :tenant and :agent_id from URL params, constructs a
// FleetScope.remote(…), and asynchronously creates a Remote-scoped
// AgentClient available via useScopedAgent() throughout the subtree.
// The Shell itself (sidebar, header, pages) is unchanged — scope-aware
// components read useScope() / useScopedAgent() to route their calls.
// ---------------------------------------------------------------------------

function ScopedShell() {
  return (
    <ScopeProvider>
      <Shell />
    </ScopeProvider>
  );
}

const router = createBrowserRouter([
  // Local agent — default scope.
  {
    path: "/",
    element: <Shell />,
    children: shellChildren,
  },
  // Remote agent scope — URL is the source of truth per FLEET-TRANSPORT.md
  // § "How Studio descends into a remote". Deep-linking, back/forward, and
  // tab isolation all work because the scope lives in the URL.
  //
  // /scope/:tenant/:agent_id/flows          → flows on remote agent
  // /scope/:tenant/:agent_id/blocks         → blocks on remote agent
  // /scope/:tenant/:agent_id/               → index (flows list) on remote
  //
  // All child routes are identical to the local shell. The difference is
  // that components calling useScopedAgent() receive the Remote-scoped
  // AgentClient instead of the local singleton.
  {
    path: "/scope/:tenant/:agent_id",
    element: <ScopedShell />,
    children: shellChildren,
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}

