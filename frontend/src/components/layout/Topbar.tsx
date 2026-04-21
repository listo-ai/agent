import { useUiStore } from "@/store/ui";
import { useAuthStore } from "@/store/auth";
import { useAuth } from "@/providers/auth";
import { LogOut, Moon, Sun } from "lucide-react";
import {
  Button,
  Separator,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@listo/ui-kit";

// ---------------------------------------------------------------------------
// Topbar — pure layout shell. Business logic lives in stores / providers.
// ---------------------------------------------------------------------------

export function Topbar() {
  const { theme, setTheme } = useUiStore();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const user = useAuthStore((s) => s.user);
  const { login, logout } = useAuth();

  const toggleTheme = () => setTheme(theme === "dark" ? "light" : "dark");
  const email = user?.profile["email"] as string | undefined;

  return (
    <header
      style={{ height: "var(--topbar-height)" }}
      className="flex shrink-0 items-center justify-between border-b border-border bg-card px-4"
    >
      {/* Brand */}
      <span className="text-sm font-semibold tracking-tight">Studio</span>

      {/* Right-hand controls */}
      <div className="flex items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={toggleTheme}
              aria-label="Toggle theme"
            >
              {theme === "dark" ? <Sun size={15} /> : <Moon size={15} />}
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            Switch to {theme === "dark" ? "light" : "dark"} mode
          </TooltipContent>
        </Tooltip>

        {isAuthenticated ? (
          <>
            <Separator orientation="vertical" className="mx-1.5 h-4" />
            <span className="text-xs text-muted-foreground">{email}</span>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  onClick={() => void logout()}
                  aria-label="Sign out"
                >
                  <LogOut size={15} />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom">Sign out</TooltipContent>
            </Tooltip>
          </>
        ) : (
          <Button size="xs" onClick={() => void login()}>
            Sign in
          </Button>
        )}
      </div>
    </header>
  );
}
