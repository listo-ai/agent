/**
 * TagsEditor — reusable `config.tags` editor.
 *
 * Pair with `useNodeTags` for full load/save behaviour, or drive it
 * directly with the `tags` + action props for controlled use (e.g. forms).
 *
 * UX:
 *   - Type a label name and press Enter / comma to add.
 *   - Type `key:value` and press Enter / comma to add a KV tag.
 *   - Click × on any chip to remove it.
 *   - Backspace on empty input removes the last label.
 */
import { useRef, useState, type KeyboardEvent } from "react";
import { X, Check, AlertCircle, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import type { NodeTagsState, TagsSaveState } from "./types";

export interface TagsEditorProps
  extends Pick<NodeTagsState, "tags" | "addLabel" | "removeLabel" | "setKv" | "removeKv"> {
  saveState?: TagsSaveState;
  saveError?: string | null;
  className?: string | undefined;
  disabled?: boolean | undefined;
}

export function TagsEditor({
  tags,
  addLabel,
  removeLabel,
  setKv,
  removeKv,
  saveState = "idle",
  saveError,
  className,
  disabled,
}: TagsEditorProps) {
  const [input, setInput] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  function commit(raw: string) {
    const text = raw.trim();
    if (!text) return;
    if (text.includes(":")) {
      const colonIdx = text.indexOf(":");
      const k = text.slice(0, colonIdx);
      const v = text.slice(colonIdx + 1);
      if (k && v) setKv(k, v);
    } else {
      addLabel(text);
    }
    setInput("");
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      commit(input);
    } else if (e.key === "Backspace" && input === "") {
      e.preventDefault();
      if (tags.labels.length > 0) {
        const last = tags.labels[tags.labels.length - 1];
        if (last) removeLabel(last);
      } else {
        const keys = Object.keys(tags.kv);
        const lastKey = keys[keys.length - 1];
        if (lastKey) removeKv(lastKey);
      }
    }
  }

  const kvEntries = Object.entries(tags.kv);
  const hasAny = tags.labels.length > 0 || kvEntries.length > 0;

  return (
    <div className={cn("space-y-2", className)}>
      {/* Chip list */}
      <div
        className={cn(
          "flex min-h-[38px] flex-wrap gap-1.5 rounded-md border border-input bg-background px-2.5 py-1.5",
          "focus-within:border-ring focus-within:ring-[3px] focus-within:ring-ring/50",
          disabled && "pointer-events-none opacity-50",
        )}
        onClick={() => inputRef.current?.focus()}
      >
        {tags.labels.map((label) => (
          <LabelChip key={label} label={label} onRemove={() => removeLabel(label)} />
        ))}
        {kvEntries.map(([k, v]) => (
          <KvChip key={k} tagKey={k} tagValue={v} onRemove={() => removeKv(k)} />
        ))}
        <input
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          onBlur={() => commit(input)}
          disabled={disabled}
          placeholder={hasAny ? "" : "Add label or key:value…"}
          className="min-w-[120px] flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        />
      </div>

      {/* Status row */}
      <div className="flex items-center gap-1.5 text-[11px]">
        {saveState === "saving" && (
          <><Loader2 size={11} className="animate-spin text-muted-foreground" />
          <span className="text-muted-foreground">Saving…</span></>
        )}
        {saveState === "ok" && (
          <><Check size={11} className="text-emerald-500" />
          <span className="text-emerald-600">Saved</span></>
        )}
        {saveState === "error" && (
          <><AlertCircle size={11} className="text-destructive" />
          <span className="text-destructive">{saveError ?? "Save failed"}</span></>
        )}
        {saveState === "idle" && (
          <span className="text-muted-foreground/60">
            Enter or comma to add · key:value for metadata
          </span>
        )}
      </div>
    </div>
  );
}

function LabelChip({ label, onRemove }: { label: string; onRemove: () => void }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-secondary px-2 py-0.5 text-xs font-medium text-secondary-foreground">
      {label}
      <button
        type="button"
        onClick={(e) => { e.stopPropagation(); onRemove(); }}
        className="ml-0.5 rounded-full opacity-50 hover:opacity-100"
        aria-label={`Remove label ${label}`}
      >
        <X size={10} />
      </button>
    </span>
  );
}

function KvChip({ tagKey, tagValue, onRemove }: { tagKey: string; tagValue: string; onRemove: () => void }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-800 dark:bg-blue-900/40 dark:text-blue-300">
      <span className="font-semibold">{tagKey}</span>
      <span className="opacity-60">:</span>
      <span>{tagValue}</span>
      <button
        type="button"
        onClick={(e) => { e.stopPropagation(); onRemove(); }}
        className="ml-0.5 rounded-full opacity-50 hover:opacity-100"
        aria-label={`Remove tag ${tagKey}`}
      >
        <X size={10} />
      </button>
    </span>
  );
}
