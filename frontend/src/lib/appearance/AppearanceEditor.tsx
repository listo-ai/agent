/**
 * AppearanceEditor — edit the `config.appearance` slot (icon + color).
 *
 * Icon: compact popover picker with curated Lucide icon set.
 * Color: preset swatches + custom full-range picker + hex input.
 */
import { Check, AlertCircle, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { IconPicker, DynamicIcon } from "@/lib/icon-picker";
import { ColorPicker } from "@/lib/color-picker";
import type { NodeAppearanceState, AppearanceSaveState } from "./types";

export interface AppearanceEditorProps
  extends Pick<NodeAppearanceState, "appearance" | "setIcon" | "setColor" | "clear"> {
  saveState?: AppearanceSaveState | undefined;
  saveError?: string | null | undefined;
  className?: string | undefined;
}

export function AppearanceEditor({
  appearance,
  setIcon,
  setColor,
  clear,
  saveState = "idle",
  saveError,
  className,
}: AppearanceEditorProps) {
  return (
    <div className={cn("space-y-4", className)}>
      {/* Icon row */}
      <div className="space-y-1.5">
        <label className="text-xs font-medium text-foreground">Icon</label>
        <div className="flex items-center gap-2">
          <IconPicker
            value={appearance.icon}
            onChange={(name) => {
              if (name) setIcon(name);
              else clear("icon");
            }}
          />
          {appearance.icon ? (
            <div className="flex min-w-0 flex-1 items-center gap-1.5 text-sm text-foreground">
              <DynamicIcon name={appearance.icon} className="h-4 w-4 shrink-0 text-muted-foreground" />
              <span className="truncate text-xs text-muted-foreground">{appearance.icon}</span>
            </div>
          ) : (
            <span className="text-xs text-muted-foreground">No icon selected</span>
          )}
        </div>
      </div>

      {/* Color row */}
      <div className="space-y-1.5">
        <label className="text-xs font-medium text-foreground">Accent color</label>
        <ColorPicker
          value={appearance.color}
          onChange={(color) => {
            if (color) setColor(color);
            else clear("color");
          }}
        />
      </div>

      {/* Save status */}
      <div className="flex items-center gap-1.5 text-[11px]">
        {saveState === "saving" && (
          <>
            <Loader2 size={11} className="animate-spin text-muted-foreground" />
            <span className="text-muted-foreground">Saving…</span>
          </>
        )}
        {saveState === "ok" && (
          <>
            <Check size={11} className="text-emerald-500" />
            <span className="text-emerald-600">Saved</span>
          </>
        )}
        {saveState === "error" && (
          <>
            <AlertCircle size={11} className="text-destructive" />
            <span className="text-destructive">{saveError ?? "Save failed"}</span>
          </>
        )}
      </div>
    </div>
  );
}
