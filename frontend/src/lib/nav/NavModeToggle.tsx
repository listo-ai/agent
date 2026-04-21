/**
 * NavModeToggle — a small sidebar footer button that switches between
 * admin (Studio) mode and user (nav-tree) mode.
 *
 * Pure presentational: receives current mode + callbacks as props.
 * Pair with `useNavMode` to wire up state.
 */
import { Settings2, LayoutGrid } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { NavMode } from "./useNavMode";

export interface NavModeToggleProps {
  mode: NavMode;
  onToggle: () => void;
}

export function NavModeToggle({ mode, onToggle }: NavModeToggleProps) {
  const isAdmin = mode === "admin";

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="h-7 w-full justify-start gap-2 px-2 text-xs text-muted-foreground hover:text-foreground"
          onClick={onToggle}
          aria-label={isAdmin ? "Switch to user view" : "Switch to admin view"}
        >
          {isAdmin ? (
            <>
              <LayoutGrid className="h-3.5 w-3.5" aria-hidden />
              <span className="group-data-[collapsible=icon]:hidden">Switch to user view</span>
            </>
          ) : (
            <>
              <Settings2 className="h-3.5 w-3.5" aria-hidden />
              <span className="group-data-[collapsible=icon]:hidden">Switch to admin view</span>
            </>
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="right">
        {isAdmin ? "Switch to user view" : "Switch to admin view"}
      </TooltipContent>
    </Tooltip>
  );
}
