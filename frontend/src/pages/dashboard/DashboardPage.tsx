import { LayoutDashboard } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui";

// Dashboard page — placeholder for dashboard widget composition (Milestone 6+).

export function DashboardPage() {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-center gap-3">
        <LayoutDashboard size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Dashboard</h1>
      </header>

      <div className="flex flex-1 items-center justify-center">
        <Card className="max-w-sm text-center">
          <CardHeader>
            <CardTitle>Coming soon</CardTitle>
            <CardDescription>Milestone 6+</CardDescription>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Composable dashboard widgets from extension contributions.
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
