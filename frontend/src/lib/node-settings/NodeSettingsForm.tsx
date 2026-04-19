import { type ReactNode } from "react";
import Form from "@rjsf/core";
import validator from "@rjsf/validator-ajv8";
import { cn } from "@/lib/utils";
import type { NodeSettingsState } from "./types";
import { normalizeJsonSchema } from "./normalizeJsonSchema";

// ─── Minimal RJSF prop shapes (no @rjsf/utils dep needed) ─────────────────
interface RjsfFieldProps {
  id: string;
  label: string;
  required: boolean;
  children: ReactNode;
  errors?: ReactNode;
  hidden?: boolean;
}
interface RjsfObjectProps {
  properties: { content: ReactNode; name: string }[];
  title?: string;
}
interface RjsfTitleProps { title?: string; }
interface RjsfWidgetProps {
  id: string;
  value: unknown;
  onChange: (v: unknown) => void;
  options: Record<string, unknown>;
  schema: { type?: string; default?: unknown };
  disabled?: boolean;
  readonly?: boolean;
  label?: string;
}

// ─── Shared input style ────────────────────────────────────────────────────
const inputCls =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm " +
  "focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-1";

// ─── Tailwind widget implementations ─────────────────────────────────────
function TwTitleField({ title }: RjsfTitleProps) {
  if (!title) return null;
  return (
    <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
      {title}
    </div>
  );
}

function TwObjectFieldTemplate({ properties, title }: RjsfObjectProps) {
  return (
    <div>
      {title && (
        <div className="mb-3 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          {title}
        </div>
      )}
      <div className="space-y-3">
        {properties.map((prop) => (
          <div key={prop.name}>{prop.content}</div>
        ))}
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
      {errors && <div className="text-xs text-destructive">{errors}</div>}
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

// ─── Public component ──────────────────────────────────────────────────────

interface NodeSettingsFormProps extends NodeSettingsState {
  /** Raw settings_schema from the Kind manifest. Null = no settings. */
  rawSchema: Record<string, unknown> | null | undefined;
  className?: string;
}

export function NodeSettingsForm({
  rawSchema,
  formData,
  onChange,
  saveState,
  saveError,
  className,
}: NodeSettingsFormProps) {
  const schema =
    rawSchema && typeof rawSchema === "object"
      ? normalizeJsonSchema(rawSchema)
      : null;

  if (!schema) {
    return (
      <p className={cn(
        "rounded-xl border border-dashed border-border bg-background px-3 py-4 text-sm text-muted-foreground",
        className,
      )}>
        This kind does not expose a settings schema.
      </p>
    );
  }

  return (
    <div className={cn("rounded-xl border border-border bg-background p-4", className)}>
      <Form
        schema={schema}
        formData={formData}
        validator={validator}
        onChange={(event) => onChange((event.formData ?? {}) as Record<string, unknown>)}
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

      {/* Save status indicator */}
      <div className="mt-3 h-4 text-xs">
        {saveState === "saving" && (
          <span className="text-muted-foreground">Saving…</span>
        )}
        {saveState === "ok" && (
          <span className="text-green-600">Saved ✓</span>
        )}
        {saveState === "error" && (
          <span className="text-destructive">Error: {saveError}</span>
        )}
      </div>
    </div>
  );
}
