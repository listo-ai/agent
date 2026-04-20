// Natural-language → layout panel. Calls the agent's
// `POST /api/v1/ui/compose` endpoint; the API key lives on the
// server, never in the browser. The panel shows the generated JSON
// + any model-emitted note; Apply writes the JSON into the editor
// buffer via `store.setLayoutText`, after which the existing
// autosave + dry-run + preview pipeline takes over.
//
// If the agent doesn't have ANTHROPIC_API_KEY configured, the
// endpoint returns a stable `compose_unavailable` error. We surface
// that with a precise hint rather than a generic 5xx.

import { useState } from "react";
import { Sparkles, X, Loader2, Check } from "lucide-react";
import { agentPromise } from "@/lib/agent";
import { useBuilderStore } from "../store/builder-store";
import type { UiComposeRequest } from "@sys/agent-client";

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

  const run = async () => {
    if (!prompt.trim() || busy) return;
    setBusy(true);
    setError(null);
    setGenerated(null);
    try {
      const client = await agentPromise;
      const req: UiComposeRequest = { prompt };
      if (draft?.layoutText) {
        try {
          req.current_layout = JSON.parse(draft.layoutText);
        } catch {
          // Buffer is not parseable JSON right now — send as a hint
          // rather than a typed object so the model can still try.
          req.context_hints = `Current editor buffer (unparseable):\n${draft.layoutText}`;
        }
      }
      const result = await client.ui.compose(req);
      const json = JSON.stringify(result.layout, null, 2);
      setGenerated(result.note ? { json, note: result.note } : { json });
    } catch (err) {
      setError(friendlyError(err));
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

      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">
            What would you like to build or change?
          </span>
          <textarea
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            disabled={busy}
            placeholder={
              draft?.layoutText?.trim()
                ? "e.g. Add a severity filter chip above the table"
                : "e.g. A dashboard for /flow-1/heartbeat with a KPI for out.payload.count and a chart of out.payload.count over time"
            }
            rows={4}
            className="w-full rounded-md border border-border bg-card p-2 text-sm disabled:opacity-50"
          />
        </label>

        <button
          type="button"
          onClick={run}
          disabled={busy || !prompt.trim()}
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

/**
 * Tease an actionable message out of the raw ClientError thrown by
 * the HTTP transport. The agent's compose handler returns stable
 * error codes in the body; we match on them for hint quality.
 */
function friendlyError(err: unknown): string {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes("compose_unavailable") || msg.includes("ANTHROPIC_API_KEY")) {
    return "Compose is disabled on this agent. Set ANTHROPIC_API_KEY in the agent's env and restart.";
  }
  if (msg.includes("upstream_error")) {
    return `The model call failed. ${msg}`;
  }
  return msg;
}
