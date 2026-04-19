import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { Shell } from "@/components/layout/Shell";
import { FlowsListPage } from "@/pages/flows/FlowsListPage";
import { FlowsPage } from "@/pages/flows/FlowsPage";
import { DashboardPage } from "@/pages/dashboard/DashboardPage";
import { ExtensionsPage } from "@/pages/extensions/ExtensionsPage";
import { PluginsPage } from "@/pages/plugins/PluginsPage";
import { SettingsPage } from "@/pages/settings/SettingsPage";

const router = createBrowserRouter([
  {
    path: "/",
    element: <Shell />,
    children: [
      { index: true, element: <FlowsListPage /> },
      { path: "flows", element: <FlowsListPage /> },
      // `*` splat captures the flow's graph path (which contains `/`).
      // Example: /flows/edit/flow-1 → flow path "/flow-1".
      { path: "flows/edit/*", element: <FlowsPage /> },
      // Short-form deep-link: /flows/flow-1 → canvas for "/flow-1".
      // Matched after flows/edit/* so the edit prefix still wins.
      { path: "flows/*", element: <FlowsPage /> },
      { path: "dashboard", element: <DashboardPage /> },
      { path: "extensions", element: <ExtensionsPage /> },
      { path: "plugins", element: <PluginsPage /> },
      { path: "settings", element: <SettingsPage /> },
    ],
  },
]);

export function AppRouter() {
  return <RouterProvider router={router} />;
}
