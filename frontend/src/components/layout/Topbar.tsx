import { useUiStore } from "@/store/ui";
import { useAuthStore } from "@/store/auth";
import { useAuth } from "@/providers/auth";
import { LogOut, Moon, Sun } from "lucide-react";

export function Topbar() {
  const { theme, setTheme } = useUiStore();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const user = useAuthStore((s) => s.user);
  const { login, logout } = useAuth();

  function toggleTheme() {
    setTheme(theme === "dark" ? "light" : "dark");
  }

  return (
    <header
      style={{ height: "var(--topbar-height)" }}
      className="flex shrink-0 items-center justify-between border-b border-border bg-card px-4"
    >
      {/* Brand */}
      <span className="text-sm font-semibold tracking-tight">Studio</span>

      {/* Right-hand controls */}
      <div className="flex items-center gap-2">
        <button
          onClick={toggleTheme}
          className="rounded-md p-1.5 text-muted-foreground hover:text-foreground"
          aria-label="Toggle theme"
        >
          {theme === "dark" ? <Sun size={15} /> : <Moon size={15} />}
        </button>

        {isAuthenticated ? (
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">
              {user?.profile["email"] as string | undefined}
            </span>
            <button
              onClick={() => void logout()}
              className="rounded-md p-1.5 text-muted-foreground hover:text-foreground"
              aria-label="Sign out"
            >
              <LogOut size={15} />
            </button>
          </div>
        ) : (
          <button
            onClick={() => void login()}
            className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:opacity-90"
          >
            Sign in
          </button>
        )}
      </div>
    </header>
  );
}
