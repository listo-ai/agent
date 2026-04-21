import { Link, useLocation, useNavigate } from "react-router-dom";
import { useUiStore } from "@/store/ui";
import { useAuthStore } from "@/store/auth";
import { useAuth } from "@/providers/auth";
import { ArrowLeft, LogOut, Moon, Sparkles, Sun } from "lucide-react";
import { useGlobalAiChat } from "@/features/global-ai-chat";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  Button,
  Separator,
  SidebarTrigger,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@listo/ui-kit";

// ---------------------------------------------------------------------------
// Route → breadcrumb segments
// ---------------------------------------------------------------------------

interface Segment {
  label: string;
  href?: string;
}

function useBreadcrumbs(): Segment[] {
  const { pathname } = useLocation();
  const parts = pathname.split("/").filter(Boolean);

  if (parts.length === 0) return [{ label: "Studio" }];

  const segments: Segment[] = [{ label: "Studio", href: "/" }];

  if (parts[0] === "flows") {
    if (parts.length === 1) {
      segments.push({ label: "Flows" });
    } else if (parts[1] === "edit") {
      // /flows/edit/<...path segments> — each segment navigable
      segments.push({ label: "Flows", href: "/flows" });
      const flowParts = parts.slice(2);
      for (let i = 0; i < flowParts.length; i++) {
        const label = "/" + flowParts.slice(0, i + 1).join("/");
        const isLast = i === flowParts.length - 1;
        if (isLast) {
          segments.push({ label });
        } else {
          segments.push({ label, href: "/flows/edit/" + flowParts.slice(0, i + 1).join("/") });
        }
      }
    } else {
      segments.push({ label: "Flows", href: "/flows" });
      segments.push({ label: parts.slice(1).join("/") });
    }
  } else if (parts[0] === "pages") {
    segments.push({ label: "Pages" });
  } else if (parts[0] === "blocks") {
    segments.push({ label: "Blocks" });
  } else if (parts[0] === "settings") {
    segments.push({ label: "Settings" });
  } else if (parts[0] === "ui") {
    // /ui/:pageRef — the ref is a UUID; don't display it. Link back to Pages.
    segments.push({ label: "Pages", href: "/pages" });
    segments.push({ label: "Page" });
  } else {
    // fallback: capitalise each segment
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i] ?? "";
      const label = part.charAt(0).toUpperCase() + part.slice(1);
      if (i < parts.length - 1) {
        segments.push({ label, href: "/" + parts.slice(0, i + 1).join("/") });
      } else {
        segments.push({ label });
      }
    }
  }

  return segments;
}

// ---------------------------------------------------------------------------
// SiteHeader
// ---------------------------------------------------------------------------

export function SiteHeader() {
  const breadcrumbs = useBreadcrumbs();
  const navigate = useNavigate();
  const { theme, setTheme } = useUiStore();
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const user = useAuthStore((s) => s.user);
  const { login, logout } = useAuth();

  const toggleTheme = () => setTheme(theme === "dark" ? "light" : "dark");
  const toggleAiChat = useGlobalAiChat((s) => s.toggle);
  const aiChatOpen = useGlobalAiChat((s) => s.open);
  const email = user?.profile["email"] as string | undefined;
  const canGoBack = breadcrumbs.length > 1;

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
      {/* Sidebar toggle */}
      <SidebarTrigger className="-ml-1" />
      <Separator orientation="vertical" className="mr-1 h-4" />

      {/* Back */}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={() => navigate(-1)}
            disabled={!canGoBack}
            aria-label="Back"
          >
            <ArrowLeft size={15} />
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom">Back</TooltipContent>
      </Tooltip>

      {/* Breadcrumbs */}
      <Breadcrumb className="flex-1">
        <BreadcrumbList>
          {breadcrumbs.map((seg, idx) => {
            const isLast = idx === breadcrumbs.length - 1;
            return (
              <BreadcrumbItem key={idx}>
                {!isLast ? (
                  <>
                    <BreadcrumbLink asChild className="text-sm">
                      <Link to={seg.href ?? "/"}>{seg.label}</Link>
                    </BreadcrumbLink>
                    <BreadcrumbSeparator />
                  </>
                ) : (
                  <BreadcrumbPage className="text-sm">{seg.label}</BreadcrumbPage>
                )}
              </BreadcrumbItem>
            );
          })}
        </BreadcrumbList>
      </Breadcrumb>

      {/* Right-hand controls */}
      <div className="flex items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={aiChatOpen ? "secondary" : "ghost"}
              size="icon-xs"
              onClick={toggleAiChat}
              aria-label="AI assistant"
              aria-pressed={aiChatOpen}
            >
              <Sparkles size={15} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">AI assistant</TooltipContent>
        </Tooltip>
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
