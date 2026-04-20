// Natural-language → layout panel. Slides in from the right; drives
// `composeLayout`, shows the generated JSON + any model-emitted note,
// lets the user apply it to the editor (autosave + preview take over
// from there) or discard.
//
// Deliberately decoupled from the autosave path: composing does not
// write to the server; apply just rewrites the editor buffer. If the
// user hits Apply and doesn't like it, Ctrl+Z in Monaco undoes.

import { useState } from "react";
import { Sparkles, X, Loader2, Check } from "lucide-react";
import { useBuilderStore } from "../store/builder-store";
import { composeLayout, hasApiKey } from "../persistence/compose-client";

interface Props {
  open: boolean;
  onClose(): void;
}

export function ComposePanel({ open, onClose }: Props) {
  const draft = useBuilderStore((s) => s.draft);
  const setLayoutText = useBuilderStore((s) => s.setLayoutText);

  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<{
    json: string;
    note?: string;
  } | null>(null);

  if (!open) return null;

  const missingKey = !hasApiKey();

  const run = async () => {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    setError(null);
    setGenerated(null);
    try {
      const req: Parameters<typeof composeLayout>[0] = { prompt };
      if (draft?.layoutText) req.currentLayout = draft.layoutText;
      const result = await composeLayout(req);
      const json = JSON.stringify(result.layout, null, 2);
      setGenerated(result.note ? { json, note: result.note } : { json });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const apply = () => {
    if (!generated) return;
    setLayoutText(generated.json);
    setGenerated(null);
    setPrompt("");
  };

  return (
    <aside
      role="dialog"
      aria-label="Compose with AI"
      className="absolute right-0 top-0 z-40 flex h-full w-[420px] flex-col border-l border-border bg-background shadow-xl"
    >
      <header className="flex items-center gap-2 border-b border-border px-4 py-2">
        <Sparkles size={14} className="text-primary" />
        <h2 className="text-sm font-semibold">Compose</h2>
        <button
          type="button"
          onClick={onClose}
          className="ml-auto rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="Close"
        >
          <X size={14} />
        </button>
      </header>

      {missingKey && (
        <div className="border-b border-amber-500/50 bg-amber-500/10 px-4 py-2 text-xs text-amber-600 dark:text-amber-400">
          Set <code>PUBLIC_ANTHROPIC_API_KEY</code> in your frontend env to
          use Compose. Restart <code>pnpm dev</code> after changing it.
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">
            What would you like to build or change?
          </span>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            disabled={busy || missingKey}
            placeholder={
              draft?.layoutText?.trim()
                ? "e.g. Add a severity filter chip above the table"
                : "e.g. A dashboard for the heartbeat at /flow-1/heartbeat with a KPI for count and a line chart of count over time"
            }
            rows={4}
            className="w-full rounded-md border border-border bg-card p-2 text-sm disabled:opacity-50"
          />
        </label>

        <button
          type="button"
          onClick={run}
          disabled={busy || missingKey || !prompt.trim()}
          className="flex items-center justify-center gap-2 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        >
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
          {busy ? "Composing…" : "Compose"}
        </button>

        {error && (
          <div
            className="rounded-md border border-destructive/50 bg-destructive/10 p-2 text-xs text-destructive"
            role="alert"
          >
            {error}
          </div>
        )}

        {generated && (
          <div className="flex flex-col gap-2">
            {generated.note && (
              <p className="whitespace-pre-wrap rounded-md border border-border bg-muted/30 p-2 text-xs text-muted-foreground">
                {generated.note}
              </p>
            )}
            <pre className="max-h-64 overflow-auto rounded-md border border-border bg-muted/20 p-2 font-mono text-[11px] leading-relaxed">
              {generated.json}
            </pre>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={apply}
                className="flex flex-1 items-center justify-center gap-1 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
              >
                <Check size={12} /> Apply to editor
              </button>
              <button
                type="button"
                onClick={() => setGenerated(null)}
                className="rounded-md border border-border px-3 py-1.5 text-xs hover:bg-accent"
              >
                Discard
              </button>
            </div>
          </div>
        )}
      </div>
    </aside>
  );
}
