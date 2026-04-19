import { Settings } from "lucide-react";
import { useUiStore } from "@/store/ui";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Label,
  Separator,
} from "@/components/ui";

// ---------------------------------------------------------------------------
// Settings page — presentational shell with theme controls.
// More sections (auth, agent, extensions) will be added in later milestones.
// ---------------------------------------------------------------------------

export function SettingsPage() {
  const { theme, setTheme } = useUiStore();

  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto p-6">
      <header className="flex items-center gap-3">
        <Settings size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Settings</h1>
      </header>

      {/* Appearance */}
      <Card>
        <CardHeader>
          <CardTitle>Appearance</CardTitle>
          <CardDescription>Manage how Studio looks.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <Label>Theme</Label>
          <div className="flex gap-2">
            {(["light", "dark", "system"] as const).map((t) => (
              <Button
                key={t}
                variant={theme === t ? "default" : "outline"}
                size="sm"
                onClick={() => setTheme(t)}
              >
                {t.charAt(0).toUpperCase() + t.slice(1)}
              </Button>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Future sections */}
      <Card>
        <CardHeader>
          <CardTitle>Agent connection</CardTitle>
          <CardDescription>Milestone 4+</CardDescription>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            Configure the agent endpoint, authentication, and extension management.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
