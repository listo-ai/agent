import { useEffect, useMemo, useState } from "react";
import Form from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import type { FieldTemplateProps, ObjectFieldTemplateProps, TitleFieldProps, WidgetProps } from "@rjsf/utils";
import type { Kind, NodeSnapshot, Slot } from "@acme/agent-client";
import { cn } from "@/lib/utils";
import { formatLiveValue, mergedSlots } from "../flow-model";

interface FlowPropertyPanelProps {
  node: NodeSnapshot | undefined;
  kind: Kind | undefined;
  /** Already a Slot map (name → {name, value, generation}) from the live feed. */
  live: Record<string, Slot>;
  onSaveConfig: (path: string, config: Record<string, unknown>) => void;
}

export function FlowPropertyPanel({
  node,
  kind,
  live,
  onSaveConfig,
}: FlowPropertyPanelProps) {
  const [formData, setFormData] = useState<Record<string, unknown>>({});

  useEffect(() => {
    setFormData({});
  }, [node?.path]);

  const schema = useMemo(() => {
    if (!kind || typeof kind.settings_schema !== "object" || kind.settings_schema === null) {
      return null;
    }
    return normalizeJsonSchema(kind.settings_schema as Record<string, unknown>);
  }, [kind]);

  const statusSlots = useMemo(() => {
    if (!node) return [];
    // `live` is already Record<string, Slot> — pass directly as LiveNodeState.slots
    return mergedSlots(node, { lifecycle: undefined, slots: live, touchedAt: undefined });
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
              <p className="mb-4 text-xs text-muted-foreground">
                Starts from schema defaults (read-back not yet implemented).
              </p>
              <Form
                schema={schema}
                formData={formData}
                validator={validator}
                onChange={(event) => {
                  const next = (event.formData ?? {}) as Record<string, unknown>;
                  setFormData(next);
                  onSaveConfig(node.path, next);
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
                  <div className="text-xs font-medium">{slot.name}</div>
                  <div className="mt-1 text-sm text-muted-foreground">{formatLiveValue(slot.value)}</div>
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </aside>
  );
}

// ─── RJSF Tailwind templates ────────────────────────────────────────────────

const inputCls =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1";

function TwTitleField({ title }: TitleFieldProps) {
  if (!title) return null;
  return <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</div>;
}

function TwObjectFieldTemplate({ properties, title }: ObjectFieldTemplateProps) {
  return (
    <div>
      {title && (
        <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">{title}</div>
      )}
      <div className="space-y-3">
        {properties.map((prop) => prop.content)}
      </div>
    </div>
  );
}

function TwFieldTemplate({ id, label, required, children, errors, hidden }: FieldTemplateProps) {
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

function TwTextWidget({ id, value, onChange, options, schema, disabled, readonly }: WidgetProps) {
  const type = schema.type === "number" || schema.type === "integer" ? "number" : "text";
  return (
    <input
      id={id}
      type={type}
      className={inputCls}
      value={value ?? ""}
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

function TwSelectWidget({ id, value, onChange, options, disabled, readonly }: WidgetProps) {
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

function TwCheckboxWidget({ id, value, onChange, label, disabled, readonly }: WidgetProps) {
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
