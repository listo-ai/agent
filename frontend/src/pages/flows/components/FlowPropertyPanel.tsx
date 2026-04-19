import { type ReactNode } from "react";
import type { Kind, NodeSnapshot, Slot } from "@acme/agent-client";
import { cn } from "@/lib/utils";
import { useNodeSettings, NodeSettingsForm } from "@/lib/node-settings";
import { mergedSlots } from "../flow-model";

/** Slot names managed by the canvas — hide from the live slots list. */
const INTERNAL_SLOTS = new Set(["position", "notes", "settings"]);

interface FlowPropertyPanelProps {
  node: NodeSnapshot | undefined;
  kind: Kind | undefined;
  /** Slot map (name → {name, value, generation}) from the live feed. */
  live: Record<string, Slot>;
  onSaveSettings: (path: string, settings: Record<string, unknown>) => Promise<void>;
}

export function FlowPropertyPanel({
  node,
  kind,
  live,
  onSaveSettings,
}: FlowPropertyPanelProps) {
  const settingsState = useNodeSettings(
    node?.path,
    live["settings"],
    onSaveSettings,
  );

  const statusSlots = node
    ? mergedSlots(node, { lifecycle: undefined, slots: live, touchedAt: undefined }).filter(
        (slot) => !INTERNAL_SLOTS.has(slot.name),
      )
    : [];

  if (!node) {
    return (
      <aside className="flex h-full w-96 shrink-0 items-center justify-center border-l border-border bg-card/70 p-6">
        <div className="max-w-xs text-center">
          <h2 className="text-sm font-semibold">Nothing selected</h2>
          <p className="mt-2 text-sm text-muted-foreground">
            Select a node on the canvas to inspect its slots and edit settings.
          </p>
        </div>
      </aside>
    );
  }

  const rawSchema =
    kind && typeof kind.settings_schema === "object" && kind.settings_schema !== null
      ? (kind.settings_schema as Record<string, unknown>)
      : null;

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-border bg-card/70">
      {/* Header */}
      <div className="border-b border-border px-5 py-4">
        <h2 className="text-sm font-semibold">{node.path.split("/").pop()}</h2>
        <p className="mt-1 text-xs text-muted-foreground">{node.path}</p>
        <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
          <span className="rounded-full bg-secondary px-2 py-0.5">{node.kind}</span>
          <span className="rounded-full bg-secondary px-2 py-0.5">{node.lifecycle}</span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-auto px-5 py-4">
        {/* Settings */}
        <section>
          <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Settings
          </h3>
          <NodeSettingsForm
            rawSchema={rawSchema}
            className="mt-3"
            {...settingsState}
          />
        </section>

        {/* Live slots */}
        <section className="mt-6">
          <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Live slots
          </h3>
          <div className="mt-3 space-y-2">
            {statusSlots.length === 0 ? (
              <p className="rounded-xl border border-dashed border-border bg-background px-3 py-4 text-sm text-muted-foreground">
                No slots published yet.
              </p>
            ) : (
              statusSlots.map((slot) => (
                <div
                  key={slot.name}
                  className="rounded-xl border border-border bg-background px-3 py-2"
                >
                  <div className="text-xs font-medium text-foreground">{slot.name}</div>
                  <SlotValue value={slot.value} />
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </aside>
  );
}

// ─── Syntax-coloured JSON renderer ──────────────────────────────────────────

function JsonValue({ value, indent = 0 }: { value: unknown; indent?: number }) {
  const pad = "  ".repeat(indent);
  const padClose = "  ".repeat(Math.max(0, indent - 1));

  if (value === null) return <span className="text-slate-400">null</span>;
  if (value === undefined) return <span className="text-slate-400">undefined</span>;
  if (typeof value === "boolean") return <span className="text-violet-500">{String(value)}</span>;
  if (typeof value === "number") return <span className="text-amber-500">{String(value)}</span>;
  if (typeof value === "string") return <span className="text-green-600">&quot;{value}&quot;</span>;

  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="text-muted-foreground">[]</span>;
    return (
      <span>
        <span className="text-muted-foreground">[</span>
        {value.map((item, i) => (
          <span key={i} className="block">
            <span className="select-none text-muted-foreground/40">{pad}</span>
            <JsonValue value={item} indent={indent + 1} />
            {i < value.length - 1 && <span className="text-muted-foreground">,</span>}
          </span>
        ))}
        <span className="text-muted-foreground">{padClose}]</span>
      </span>
    );
  }

  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.length === 0) return <span className="text-muted-foreground">{"{}"}</span>;
    return (
      <span>
        <span className="text-muted-foreground">{"{"}</span>
        {entries.map(([k, v], i) => (
          <span key={k} className="block">
            <span className="select-none text-muted-foreground/40">{pad}</span>
            <span className="text-sky-600">&quot;{k}&quot;</span>
            <span className="text-muted-foreground">: </span>
            <JsonValue value={v} indent={indent + 1} />
            {i < entries.length - 1 && <span className="text-muted-foreground">,</span>}
          </span>
        ))}
        <span className="text-muted-foreground">{padClose}{"}"}</span>
      </span>
    );
  }

  return <span className="text-muted-foreground">{String(value)}</span>;
}

function SlotValue({ value }: { value: unknown }) {
  const isComplex = typeof value === "object" && value !== null;
  return (
    <pre
      className={cn(
        "mt-1 overflow-x-auto font-mono leading-relaxed",
        isComplex ? "text-xs" : "text-sm",
      )}
    >
      <JsonValue value={value} indent={1} />
    </pre>
  );
}

// keep ReactNode import live (used implicitly by JSX)
type _Unused = ReactNode;
