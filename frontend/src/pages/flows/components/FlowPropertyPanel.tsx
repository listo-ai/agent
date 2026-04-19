import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import Form from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type { Kind, NodeSnapshot, Slot } from "@acme/agent-client";
import { cn } from "@/lib/utils";
import { mergedSlots } from "../flow-model";

// Minimal prop shapes extracted from @rjsf/core internals — avoids needing @rjsf/utils as a direct dep.
interface RjsfFieldProps { id: string; label: string; required: boolean; children: ReactNode; errors?: ReactNode; hidden?: boolean; }
interface RjsfObjectProps { properties: { content: ReactNode; name: string }[]; title?: string; }
interface RjsfTitleProps { title?: string; }
interface RjsfWidgetProps { id: string; value: unknown; onChange: (v: unknown) => void; options: Record<string, unknown>; schema: { type?: string; default?: unknown }; disabled?: boolean; readonly?: boolean; label?: string; }

/** Slot names managed by the canvas — hide them from the live slots list. */
const INTERNAL_SLOTS = new Set(["position", "notes", "settings"]);

function prettyValue(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "—";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** Is the value complex enough to want a <pre> block? */
function isComplex(value: unknown): boolean {
  return typeof value === "object" && value !== null;
}

interface FlowPropertyPanelProps {
  node: NodeSnapshot | undefined;
  kind: Kind | undefined;
  /** Already a Slot map (name → {name, value, generation}) from the live feed. */
  live: Record<string, Slot>;
  onSaveSettings: (path: string, settings: Record<string, unknown>) => Promise<void>;
}

export function FlowPropertyPanel({
  node,
  kind,
  live,
  onSaveSettings,
}: FlowPropertyPanelProps) {
  const [formData, setFormData] = useState<Record<string, unknown>>({});
  const [saveState, setSaveState] = useState<"idle" | "saving" | "ok" | "error">("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  // When node changes, seed form from the live `settings` slot if present.
  useEffect(() => {
    const settingsValue = live["settings"]?.value;
    if (settingsValue !== null && settingsValue !== undefined && typeof settingsValue === "object" && !Array.isArray(settingsValue)) {
      setFormData(settingsValue as Record<string, unknown>);
    } else {
      setFormData({});
    }
    setSaveState("idle");
    setSaveError(null);
  }, [node?.path]); // eslint-disable-line react-hooks/exhaustive-deps

  // Also update form when the settings slot arrives from live data (first load).
  const settingsGeneration = live["settings"]?.generation;
  useEffect(() => {
    const settingsValue = live["settings"]?.value;
    if (settingsValue !== null && settingsValue !== undefined && typeof settingsValue === "object" && !Array.isArray(settingsValue)) {
      setFormData((prev) => {
        // Only overwrite if form is still empty (not yet touched by user).
        if (Object.keys(prev).length === 0) {
          return settingsValue as Record<string, unknown>;
        }
        return prev;
      });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settingsGeneration]);

  const handleSave = (data: Record<string, unknown>) => {
    if (!node) return;
    window.clearTimeout(saveTimer.current);
    setSaveState("saving");
    saveTimer.current = window.setTimeout(() => {
      onSaveSettings(node.path, data)
        .then(() => {
          setSaveState("ok");
          setSaveError(null);
          saveTimer.current = window.setTimeout(() => setSaveState("idle"), 2000);
        })
        .catch((err: unknown) => {
          setSaveState("error");
          setSaveError(err instanceof Error ? err.message : String(err));
        });
    }, 400);
  };

  const schema = useMemo(() => {
    if (!kind || typeof kind.settings_schema !== "object" || kind.settings_schema === null) {
      return null;
    }
    return normalizeJsonSchema(kind.settings_schema as Record<string, unknown>);
  }, [kind]);

  const statusSlots = useMemo(() => {
    if (!node) return [];
    return mergedSlots(node, { lifecycle: undefined, slots: live, touchedAt: undefined })
      .filter((slot) => !INTERNAL_SLOTS.has(slot.name));
  }, [live, node]);

  if (!node) {
    return (
      <aside className="flex h-full w-96 shrink-0 items-center justify-center border-l border-border bg-card/70 p-6">
        <div className="max-w-xs text-center">
          <h2 className="text-sm font-semibold">Nothing selected</h2>
          <p className="mt-2 text-sm text-muted-foreground">
            Select a node to inspect its slots and edit its settings schema.
          </p>
        </div>
      </aside>
    );
  }

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-border bg-card/70">
      <div className="border-b border-border px-5 py-4">
        <h2 className="text-sm font-semibold">{node.path.split("/").pop()}</h2>
        <p className="mt-1 text-xs text-muted-foreground">{node.path}</p>
        <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
          <span className="rounded-full bg-secondary px-2 py-0.5">{node.kind}</span>
          <span className="rounded-full bg-secondary px-2 py-0.5">{node.lifecycle}</span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-auto px-5 py-4">
        <section>
          <h3 className="text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Settings
          </h3>
          {schema ? (
            <div className="mt-3 rounded-xl border border-border bg-background p-4">
              <Form
                schema={schema}
                formData={formData}
                validator={validator}
                onChange={(event) => {
                  const next = (event.formData ?? {}) as Record<string, unknown>;
                  setFormData(next);
                  handleSave(next);
                }}
                showErrorList={false}
                templates={{
                  FieldTemplate: TwFieldTemplate,
                  ObjectFieldTemplate: TwObjectFieldTemplate,
                  TitleFieldTemplate: TwTitleField,
                  DescriptionFieldTemplate: () => null,
                }}
                widgets={{
                  TextWidget: TwTextWidget,
                  TextareaWidget: TwTextWidget,
                  SelectWidget: TwSelectWidget,
                  CheckboxWidget: TwCheckboxWidget,
                  UpDownWidget: TwTextWidget,
                }}
                uiSchema={{ "ui:submitButtonOptions": { norender: true } }}
              >
                <div />
              </Form>
              <div className="mt-3 flex items-center gap-2 text-xs">
                {saveState === "saving" && <span className="text-muted-foreground">Saving…</span>}
                {saveState === "ok" && <span className="text-green-600">Saved ✓</span>}
                {saveState === "error" && <span className="text-destructive">Error: {saveError}</span>}
              </div>
            </div>
          ) : (
            <p className="mt-3 rounded-xl border border-dashed border-border bg-background px-3 py-4 text-sm text-muted-foreground">
              This kind does not expose a settings schema.
            </p>
          )}
        </section>

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

// ─── Syntax-coloured JSON renderer ─────────────────────────────────────────

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
        <span className="text-muted-foreground">{'{'}</span>
        {entries.map(([k, v], i) => (
          <span key={k} className="block">
            <span className="select-none text-muted-foreground/40">{pad}</span>
            <span className="text-sky-600">&quot;{k}&quot;</span>
            <span className="text-muted-foreground">: </span>
            <JsonValue value={v} indent={indent + 1} />
            {i < entries.length - 1 && <span className="text-muted-foreground">,</span>}
          </span>
        ))}
        <span className="text-muted-foreground">{padClose}{'}'}</span>
      </span>
    );
  }

  return <span className="text-muted-foreground">{String(value)}</span>;
}

function SlotValue({ value }: { value: unknown }) {
  const isComplex = typeof value === "object" && value !== null;
  return (
    <pre className={cn(
      "mt-1 overflow-x-auto font-mono leading-relaxed",
      isComplex ? "text-xs" : "text-sm",
    )}>
      <JsonValue value={value} indent={1} />
    </pre>
  );
}

const inputCls =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1";

function TwTitleField({ title }: RjsfTitleProps) {
  if (!title) return null;
  return <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</div>;
}

function TwObjectFieldTemplate({ properties, title }: RjsfObjectProps) {
  return (
    <div>
      {title && (
        <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</div>
      )}
      <div className="space-y-3">
        {properties.map((prop) => <div key={prop.name}>{prop.content}</div>)}
      </div>
    </div>
  );
}

function TwFieldTemplate({ id, label, required, children, errors, hidden }: RjsfFieldProps) {
  if (hidden) return <>{children}</>;
  return (
    <div className="flex flex-col gap-1">
      {label && (
        <label htmlFor={id} className="text-xs font-medium text-foreground">
          {label}
          {required && <span className="ml-0.5 text-destructive">*</span>}
        </label>
      )}
      {children}
      {errors && (
        <div className="text-xs text-destructive">{errors}</div>
      )}
    </div>
  );
}

function TwTextWidget({ id, value, onChange, options, schema, disabled, readonly }: RjsfWidgetProps) {
  const type = schema.type === "number" || schema.type === "integer" ? "number" : "text";
  return (
    <input
      id={id}
      type={type}
      className={inputCls}
      value={(value as string | number) ?? ""}
      disabled={disabled || readonly}
      placeholder={String(options.placeholder ?? schema.default ?? "")}
      onChange={(e) => {
        const raw = e.target.value;
        if (type === "number") {
          const n = Number(raw);
          onChange(raw === "" ? undefined : Number.isNaN(n) ? raw : n);
        } else {
          onChange(raw === "" ? undefined : raw);
        }
      }}
    />
  );
}

function TwSelectWidget({ id, value, onChange, options, disabled, readonly }: RjsfWidgetProps) {
  const enumOptions = (options.enumOptions ?? []) as { value: unknown; label: string }[];
  return (
    <select
      id={id}
      className={cn(inputCls, "cursor-pointer")}
      value={String(value ?? "")}
      disabled={disabled || readonly}
      onChange={(e) => {
        const opt = enumOptions.find((o) => String(o.value) === e.target.value);
        onChange(opt ? opt.value : e.target.value);
      }}
    >
      {enumOptions.map((opt) => (
        <option key={String(opt.value)} value={String(opt.value)}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

function TwCheckboxWidget({ id, value, onChange, label, disabled, readonly }: RjsfWidgetProps) {
  return (
    <label htmlFor={id} className="flex items-center gap-2 text-sm">
      <input
        id={id}
        type="checkbox"
        className="h-4 w-4 rounded border-input accent-primary"
        checked={Boolean(value)}
        disabled={disabled || readonly}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label && <span className="text-foreground">{label}</span>}
    </label>
  );
}

// ────────────────────────────────────────────────────────────────────────────

function normalizeJsonSchema(schema: Record<string, unknown>): Record<string, unknown> {
  const normalized: Record<string, unknown> = { ...schema };

  const explicitType = normalized["type"];
  if (explicitType === undefined) {
    const inferred = inferSchemaType(normalized);
    if (inferred !== undefined) {
      normalized["type"] = inferred;
    }
  }

  const properties = normalized["properties"];
  if (isRecord(properties)) {
    normalized["properties"] = Object.fromEntries(
      Object.entries(properties).map(([key, value]) => [
        key,
        isRecord(value) ? normalizeJsonSchema(value) : value,
      ]),
    );
  }

  const items = normalized["items"];
  if (isRecord(items)) {
    normalized["items"] = normalizeJsonSchema(items);
  }

  const oneOf = normalizeSchemaArray(normalized["oneOf"]);
  if (oneOf) normalized["oneOf"] = oneOf;

  const anyOf = normalizeSchemaArray(normalized["anyOf"]);
  if (anyOf) normalized["anyOf"] = anyOf;

  const allOf = normalizeSchemaArray(normalized["allOf"]);
  if (allOf) normalized["allOf"] = allOf;

  return normalized;
}

function normalizeSchemaArray(value: unknown): unknown[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }

  return value.map((entry) => (isRecord(entry) ? normalizeJsonSchema(entry) : entry));
}

function inferSchemaType(schema: Record<string, unknown>): string | string[] | undefined {
  if ("properties" in schema) {
    return "object";
  }
  if ("items" in schema) {
    return "array";
  }
  if (!("default" in schema)) {
    return undefined;
  }

  const fallback = inferTypeFromValue(schema["default"]);
  if (fallback === "null") {
    return ["null", "string"];
  }
  return fallback;
}

function inferTypeFromValue(value: unknown): string | undefined {
  if (value === null) return "null";
  if (typeof value === "string") return "string";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "number") return Number.isInteger(value) ? "integer" : "number";
  if (Array.isArray(value)) return "array";
  if (isRecord(value)) return "object";
  return undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
