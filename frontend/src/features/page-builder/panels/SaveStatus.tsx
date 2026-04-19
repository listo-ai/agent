// One-line autosave status for the header strip. Reads the store
// only — no timers, no actions.

import { Check, CircleDashed, CircleX, Loader2 } from "lucide-react";
import { useBuilderStore } from "../store/builder-store";

export function SaveStatus() {
  const state = useBuilderStore((s) => s.saveState);
  switch (state.kind) {
    case "idle":
      return (
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          <CircleDashed size={12} /> idle
        </span>
      );
    case "saving":
      return (
        <span className="flex items-center gap-1 text-xs text-muted-foreground">
          <Loader2 size={12} className="animate-spin" /> saving…
        </span>
      );
    case "saved":
      return (
        <span className="flex items-center gap-1 text-xs text-emerald-500">
          <Check size={12} /> saved
        </span>
      );
    case "error":
      return (
        <span
          className="flex items-center gap-1 text-xs text-destructive"
          title={state.message}
        >
          <CircleX size={12} /> save failed
        </span>
      );
  }
}
