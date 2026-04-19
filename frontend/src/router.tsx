import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { Shell } from "@/components/layout/Shell";
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
      { index: true, element: <FlowsPage /> },
      { path: "flows", element: <FlowsPage /> },
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
