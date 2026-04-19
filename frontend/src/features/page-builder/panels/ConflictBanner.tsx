// Non-dismissable overlay shown when a concurrent write bumped the
// layout's generation past what we were editing against. Only two
// exits: reload (discard local), or copy local to clipboard. Enforces
// the OCC invariant from DASHBOARD-BUILDER.md — no "keep editing"
// escape hatch that could overwrite someone else's work.

import { AlertTriangle } from "lucide-react";
import { useBuilderStore } from "../store/builder-store";

interface Props {
  onReload(): void;
}

export function ConflictBanner({ onReload }: Props) {
  const conflict = useBuilderStore((s) => s.conflict);
  const layoutText = useBuilderStore((s) => s.draft?.layoutText ?? "");

  if (!conflict) return null;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(layoutText);
    } catch {
      // Clipboard may be denied — fall back to a transient alert so
      // the user knows nothing was lost if they proceed to reload.
      alert("Clipboard unavailable. Copy the editor contents manually before reloading.");
    }
  };

  return (
    <div
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="builder-conflict-title"
      className="absolute inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm"
    >
      <div className="max-w-md rounded-lg border border-destructive bg-card p-5 shadow-lg">
        <div className="mb-3 flex items-center gap-2">
          <AlertTriangle size={16} className="text-destructive" />
          <h2 id="builder-conflict-title" className="text-sm font-semibold">
            Another edit landed
          </h2>
        </div>
        <p className="mb-4 text-xs text-muted-foreground">
          Someone (or another process) wrote to this page while you were
          editing. Current server generation is{" "}
          <code className="rounded bg-muted px-1 py-0.5">
            {conflict.currentGeneration}
          </code>
          . Your local edits are still in the editor — copy them out before
          reloading if you want to keep them.
        </p>
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={handleCopy}
            className="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-accent"
          >
            Copy local to clipboard
          </button>
          <button
            type="button"
            onClick={onReload}
            className="rounded-md bg-destructive px-3 py-1.5 text-xs font-medium text-destructive-foreground hover:bg-destructive/90"
          >
            Reload (discard local)
          </button>
        </div>
      </div>
    </div>
  );
}
