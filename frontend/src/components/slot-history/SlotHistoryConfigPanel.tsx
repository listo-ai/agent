/**
 * SlotHistoryConfigPanel
 *
 * Lets the user pick a recording policy (COV / Interval / OnDemand) for a
 * specific slot and persist it to the agent via a `sys.core.history.config`
 * child node.
 *
 * Flow:
 *  1. Query for an existing history-config child node under `nodePath`.
 *  2. If none exists, create it on save.
 *  3. Write the `settings` slot with the merged policy map.
 */

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useAgent } from "@/hooks/useAgent";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

const HISTORY_CONFIG_KIND = "sys.core.history.config";
const HISTORY_CONFIG_NAME = "history";

// ─── Domain types ────────────────────────────────────────────────────────────

type PolicyKind = "cov" | "interval" | "on_demand";

/** Per-path historization — mirrors Rust `HistoryPath` in
 *  `crates/domain-history/src/config.rs`. `as` drives storage routing:
 *  number/bool → time-series; string/json/binary → slot_history. */
type AsType = "bool" | "number" | "string" | "json" | "binary" | "null";
const AS_TYPES: AsType[] = ["number", "bool", "string", "json", "binary", "null"];

interface HistoryPath {
  path: string;
  as: AsType;
}

interface CovFields {
  policy: "cov";
  deadband: number;
  min_interval_ms: number;
  max_gap_ms: number;
  max_samples?: number | undefined;
  paths?: HistoryPath[];
}

interface IntervalFields {
  policy: "interval";
  period_ms: number;
  align_to_wall: boolean;
  max_samples?: number | undefined;
  paths?: HistoryPath[];
}

interface OnDemandFields {
  policy: "on_demand";
  paths?: HistoryPath[];
}

type SlotPolicy = CovFields | IntervalFields | OnDemandFields;

interface HistoryConfigSettings {
  slots: Record<string, SlotPolicy>;
}

// ─── Default form values ─────────────────────────────────────────────────────

function defaultCov(): CovFields {
  return { policy: "cov", deadband: 0, min_interval_ms: 1000, max_gap_ms: 60000 };
}
function defaultInterval(): IntervalFields {
  return { policy: "interval", period_ms: 5000, align_to_wall: false };
}
function defaultOnDemand(): OnDemandFields {
  return { policy: "on_demand" };
}

function defaultForKind(kind: PolicyKind): SlotPolicy {
  if (kind === "cov") return defaultCov();
  if (kind === "interval") return defaultInterval();
  return defaultOnDemand();
}

// ─── Error helpers ────────────────────────────────────────────────────────────

function errorMessage(err: unknown): string {
  if (!err) return "";
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null) {
    const e = err as Record<string, unknown>;
    if (typeof e["message"] === "string") return e["message"] as string;
    if (typeof e["status"] === "number") return `HTTP ${e["status"] as number}`;
  }
  return String(err);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function parseNumber(s: string, fallback: number): number {
  const n = Number(s);
  return Number.isFinite(n) && n >= 0 ? n : fallback;
}

// ─── Sub-form components ──────────────────────────────────────────────────────

function FieldRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid gap-1">
      <Label className="text-xs font-medium">{label}</Label>
      {hint && <p className="text-[11px] text-muted-foreground">{hint}</p>}
      {children}
    </div>
  );
}

function CovForm({
  value,
  onChange,
}: {
  value: CovFields;
  onChange: (v: CovFields) => void;
}) {
  return (
    <div className="space-y-3">
      <FieldRow label="Deadband" hint="Minimum change before a new record is written.">
        <Input
          type="number"
          min={0}
          step="any"
          value={value.deadband}
          onChange={(e) =>
            onChange({ ...value, deadband: parseNumber(e.target.value, 0) })
          }
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
      <FieldRow label="Min interval (ms)" hint="Don't record more often than this.">
        <Input
          type="number"
          min={0}
          step={100}
          value={value.min_interval_ms}
          onChange={(e) =>
            onChange({
              ...value,
              min_interval_ms: parseNumber(e.target.value, 1000),
            })
          }
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
      <FieldRow label="Max gap (ms)" hint="Force a record if this gap passes with no change.">
        <Input
          type="number"
          min={0}
          step={1000}
          value={value.max_gap_ms}
          onChange={(e) =>
            onChange({ ...value, max_gap_ms: parseNumber(e.target.value, 60000) })
          }
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
      <FieldRow label="Max samples" hint="Optional cap on stored records (leave blank for unlimited).">
        <Input
          type="number"
          min={1}
          placeholder="unlimited"
          value={value.max_samples ?? ""}
          onChange={(e) => {
            const n = parseNumber(e.target.value, 0);
            onChange({ ...value, max_samples: e.target.value === "" ? undefined : n });
          }}
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
    </div>
  );
}

function IntervalForm({
  value,
  onChange,
}: {
  value: IntervalFields;
  onChange: (v: IntervalFields) => void;
}) {
  return (
    <div className="space-y-3">
      <FieldRow label="Period (ms)" hint="Store a sample every N milliseconds.">
        <Input
          type="number"
          min={100}
          step={1000}
          value={value.period_ms}
          onChange={(e) =>
            onChange({ ...value, period_ms: parseNumber(e.target.value, 5000) })
          }
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
      <FieldRow label="Align to wall clock" hint="Snap samples to whole-period boundaries (e.g. :00, :05, :10…).">
        <div className="flex items-center gap-2">
          <input
            type="checkbox"
            id="align-to-wall"
            checked={value.align_to_wall}
            onChange={(e) => onChange({ ...value, align_to_wall: e.target.checked })}
            className="h-4 w-4 rounded border-border"
          />
          <label htmlFor="align-to-wall" className="text-xs text-muted-foreground cursor-pointer">
            {value.align_to_wall ? "Enabled" : "Disabled"}
          </label>
        </div>
      </FieldRow>
      <FieldRow label="Max samples" hint="Optional cap on stored records (leave blank for unlimited).">
        <Input
          type="number"
          min={1}
          placeholder="unlimited"
          value={value.max_samples ?? ""}
          onChange={(e) => {
            const n = parseNumber(e.target.value, 0);
            onChange({ ...value, max_samples: e.target.value === "" ? undefined : n });
          }}
          className="h-7 font-mono text-xs"
        />
      </FieldRow>
    </div>
  );
}

/** Walk a kind's declared `value_schema` (JSON Schema) and yield every
 *  leaf path with the type the historizer should store it as. This is
 *  the authoritative source of suggestions — always available, even
 *  before the slot has emitted. Underscore-prefixed properties are
 *  skipped (platform-reserved: `_msgid` etc). Object nodes are also
 *  offered as `json` snapshots so an author can opt to record a whole
 *  subtree as structured JSON. */
function pathsFromSchema(schema: unknown, prefix = ""): HistoryPath[] {
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) return [];
  const s = schema as Record<string, unknown>;
  const t = typeof s["type"] === "string" ? (s["type"] as string) : undefined;
  switch (t) {
    case "boolean":
      return [{ path: prefix, as: "bool" }];
    case "number":
    case "integer":
      return [{ path: prefix, as: "number" }];
    case "string":
      return [{ path: prefix, as: "string" }];
    case "array":
      return [{ path: prefix, as: "json" }];
    case "object": {
      const props = s["properties"];
      if (!props || typeof props !== "object") {
        // Object declared without properties → offer the whole thing
        // as a json snapshot unless we're at the root (root = slot
        // value, already the author's scope).
        return prefix ? [{ path: prefix, as: "json" }] : [];
      }
      const out: HistoryPath[] = [];
      for (const [k, sub] of Object.entries(props as Record<string, unknown>)) {
        if (k.startsWith("_")) continue;
        const next = prefix ? `${prefix}.${k}` : k;
        out.push(...pathsFromSchema(sub, next));
      }
      if (prefix.length > 0) out.push({ path: prefix, as: "json" });
      return out;
    }
    default:
      // Property declared without a `type` keyword (e.g. trigger's
      // user-configured payload) — the manifest names it but doesn't
      // constrain it. Offer as a json snapshot; the author can switch
      // `as:` to a scalar if they know the runtime shape.
      if (prefix.length > 0) return [{ path: prefix, as: "json" }];
      return [];
  }
}

/** Fallback walker for the *live* value when a kind doesn't declare a
 *  detailed `value_schema` (e.g. trigger / function nodes emit dynamic
 *  payloads). Same output shape as `pathsFromSchema`, inferred from
 *  whatever the slot currently holds. */
function pathsFromValue(v: unknown, prefix = ""): HistoryPath[] {
  if (v === null || v === undefined) return [{ path: prefix, as: "null" }];
  if (typeof v === "boolean") return [{ path: prefix, as: "bool" }];
  if (typeof v === "number") return [{ path: prefix, as: "number" }];
  if (typeof v === "string") return [{ path: prefix, as: "string" }];
  if (Array.isArray(v)) return [{ path: prefix, as: "json" }];
  if (typeof v === "object") {
    const out: HistoryPath[] = [];
    for (const [k, sub] of Object.entries(v)) {
      if (k.startsWith("_")) continue;
      const next = prefix ? `${prefix}.${k}` : k;
      out.push(...pathsFromValue(sub, next));
    }
    if (prefix.length > 0) out.push({ path: prefix, as: "json" });
    return out;
  }
  return [];
}

function PathsEditor({
  value,
  onChange,
  suggestions,
}: {
  value: HistoryPath[];
  onChange: (v: HistoryPath[]) => void;
  suggestions: HistoryPath[];
}) {
  const taken = new Set(value.map((p) => `${p.path}:${p.as}`));
  const unused = suggestions.filter((s) => !taken.has(`${s.path}:${s.as}`));

  return (
    <div className="space-y-2 rounded-md border border-border bg-muted/30 p-3">
      <div className="flex items-baseline justify-between">
        <Label className="text-xs font-medium">Paths</Label>
        <span className="text-[11px] text-muted-foreground">
          Historize sub-fields of the slot value, each with a declared type.
        </span>
      </div>
      <p className="text-[11px] text-muted-foreground">
        Leave empty to historize the whole slot value (routed by its native
        kind). Add a path like <code className="font-mono">payload.count</code>{" "}
        with <code className="font-mono">as: number</code> to split a Msg
        envelope into a scalar time-series.
      </p>

      {unused.length > 0 && (
        <div className="space-y-1">
          <Label className="text-[11px] font-medium text-muted-foreground">
            Suggested (from current slot value)
          </Label>
          <div className="flex flex-wrap gap-1">
            {unused.map((s) => (
              <button
                key={`${s.path}:${s.as}`}
                type="button"
                onClick={() => onChange([...value, s])}
                className="inline-flex items-center gap-1 rounded-md border border-border bg-background px-2 py-0.5 text-[11px] font-mono hover:bg-accent hover:text-accent-foreground"
              >
                <span>{s.path || "(whole)"}</span>
                <span className="text-muted-foreground">→</span>
                <span className="text-foreground/70">{s.as}</span>
              </button>
            ))}
          </div>
        </div>
      )}

      {value.length > 0 && (
        <div className="space-y-1.5">
          {value.map((p, i) => (
            <div key={i} className="flex items-center gap-2">
              <Input
                placeholder="payload.count"
                value={p.path}
                onChange={(e) => {
                  const next = [...value];
                  next[i] = { ...p, path: e.target.value };
                  onChange(next);
                }}
                className="h-7 flex-1 font-mono text-xs"
              />
              <Select
                value={p.as}
                onValueChange={(v) => {
                  const next = [...value];
                  next[i] = { ...p, as: v as AsType };
                  onChange(next);
                }}
              >
                <SelectTrigger size="sm" className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {AS_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      <span className="font-mono">{t}</span>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 px-2 text-destructive hover:text-destructive"
                onClick={() => onChange(value.filter((_, j) => j !== i))}
                aria-label="Remove path"
              >
                ×
              </Button>
            </div>
          ))}
        </div>
      )}

      <Button
        type="button"
        variant="outline"
        size="sm"
        className="h-7"
        onClick={() => onChange([...value, { path: "", as: "number" }])}
      >
        + Add path
      </Button>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export interface SlotHistoryConfigPanelProps {
  /** Absolute path to the node whose slot we want to configure. */
  nodePath: string;
  /** Slot name to configure. */
  slot: string;
  onSaved?: () => void;
}

export function SlotHistoryConfigPanel({
  nodePath,
  slot,
  onSaved,
}: SlotHistoryConfigPanelProps) {
  const agent = useAgent();
  const qc = useQueryClient();

  // ── 1. Resolve config node path ──────────────────────────────────────────
  const configNodePath = `${nodePath.replace(/\/$/, "")}/${HISTORY_CONFIG_NAME}`;

  const configQuery = useQuery({
    queryKey: ["historyConfig", nodePath],
    queryFn: async (): Promise<HistoryConfigSettings | null> => {
      const client = agent.data!;
      try {
        const node = await client.nodes.getNode(configNodePath);
        const settingsSlot = node.slots?.find?.((s: { name: string }) => s.name === "settings");
        if (settingsSlot?.value != null && typeof settingsSlot.value === "object") {
          return settingsSlot.value as HistoryConfigSettings;
        }
        return { slots: {} };
      } catch {
        return null; // node doesn't exist yet
      }
    },
    enabled: agent.data !== undefined,
  });

  const existingPolicy: SlotPolicy | undefined =
    configQuery.data?.slots?.[slot];

  // Fetch this node's declared slot schemas directly.
  // `GET /api/v1/node/schema?path=…` lets the Paths editor suggest
  // concrete sub-paths (e.g. `payload.count → number`) without
  // requiring a runtime emit first.
  const nodeSchemaQuery = useQuery({
    queryKey: ["nodeSchema", nodePath],
    queryFn: async () => agent.data!.nodes.getNodeSchema(nodePath),
    enabled: agent.data !== undefined,
    staleTime: 60_000,
  });

  // Live-value fallback — used when the kind declares no strict schema
  // (function / Wasm blocks emitting dynamic shapes).
  const slotValueQuery = useQuery({
    queryKey: ["historyConfigSlotValue", nodePath, slot],
    queryFn: async (): Promise<unknown> => {
      const node = await agent.data!.nodes.getNode(nodePath);
      const s = node.slots?.find?.((x: { name: string }) => x.name === slot);
      return s?.value ?? null;
    },
    enabled: agent.data !== undefined,
    staleTime: 10_000,
  });

  const pathSuggestions: HistoryPath[] = (() => {
    // Prefer the manifest's `value_schema` — authoritative and
    // available even before the slot has emitted.
    const slotDef = nodeSchemaQuery.data?.slots.find((s) => s.name === slot);
    const fromSchema = slotDef ? pathsFromSchema(slotDef.value_schema) : [];
    if (fromSchema.length > 0) return fromSchema;

    // Fallback: probe the live value.
    return slotValueQuery.data != null ? pathsFromValue(slotValueQuery.data) : [];
  })();

  // ── 2. Local form state ───────────────────────────────────────────────────
  const [policyKind, setPolicyKind] = useState<PolicyKind>(
    (existingPolicy?.policy as PolicyKind | undefined) ?? "cov",
  );
  const [covFields, setCovFields] = useState<CovFields>(
    existingPolicy?.policy === "cov" ? existingPolicy : defaultCov(),
  );
  const [intervalFields, setIntervalFields] = useState<IntervalFields>(
    existingPolicy?.policy === "interval" ? existingPolicy : defaultInterval(),
  );
  // Per-path split (Stage 6 of NODE-RED-MODEL.md). Shared across policy
  // variants so toggling COV ↔ Interval keeps the path list intact.
  const [paths, setPaths] = useState<HistoryPath[]>(existingPolicy?.paths ?? []);

  // Sync form when query resolves
  useEffect(() => {
    if (!existingPolicy) return;
    setPolicyKind(existingPolicy.policy as PolicyKind);
    if (existingPolicy.policy === "cov") setCovFields(existingPolicy);
    if (existingPolicy.policy === "interval") setIntervalFields(existingPolicy);
    setPaths(existingPolicy.paths ?? []);
  }, [existingPolicy]);

  // ── 3. Save mutation ──────────────────────────────────────────────────────
  const saveMutation = useMutation({
    mutationFn: async () => {
      const client = agent.data!;
      // Ensure config node exists.
      let configExists = configQuery.data !== null;
      if (!configExists) {
        try {
          await client.nodes.createNode({
            parent: nodePath,
            kind: HISTORY_CONFIG_KIND,
            name: HISTORY_CONFIG_NAME,
          });
          configExists = true;
        } catch (e: unknown) {
          // 409 = already exists, treat as success
          const status = (e as { status?: number })?.status;
          if (status !== 409) throw e;
        }
      }

      // Build new settings: merge with existing, update this slot.
      const currentSettings: HistoryConfigSettings =
        configQuery.data ?? { slots: {} };
      const newPolicy = defaultForKind(policyKind);
      if (policyKind === "cov") Object.assign(newPolicy, covFields);
      if (policyKind === "interval") Object.assign(newPolicy, intervalFields);
      // Attach paths (Stage 6). Drop rows with blank path strings so
      // users can have an in-progress row without it hitting the wire.
      const cleaned = paths
        .map((p) => ({ path: p.path.trim(), as: p.as }))
        .filter((p) => p.path.length > 0 || p.as === "json" || p.as === "null");
      if (cleaned.length > 0) newPolicy.paths = cleaned;

      const newSettings: HistoryConfigSettings = {
        ...currentSettings,
        slots: {
          ...currentSettings.slots,
          [slot]: newPolicy,
        },
      };

      await client.slots.writeSlot(configNodePath, "settings", newSettings);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["historyConfig", nodePath] });
      onSaved?.();
    },
  });

  // ── 4. Remove slot policy ─────────────────────────────────────────────────
  const removeMutation = useMutation({
    mutationFn: async () => {
      if (!configQuery.data) return;
      const client = agent.data!;
      const newSlots = { ...configQuery.data.slots };
      delete newSlots[slot];
      const newSettings: HistoryConfigSettings = {
        ...configQuery.data,
        slots: newSlots,
      };
      await client.slots.writeSlot(configNodePath, "settings", newSettings);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["historyConfig", nodePath] });
    },
  });

  const isConfigured = Boolean(existingPolicy);
  const isBusy = saveMutation.isPending || removeMutation.isPending;

  return (
    <div className="space-y-4">
      {/* Policy selector */}
      <div className="grid gap-1.5">
        <Label className="text-xs font-medium">Recording policy</Label>
        <Select
          value={policyKind}
          onValueChange={(v) => setPolicyKind(v as PolicyKind)}
        >
          <SelectTrigger size="sm" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="cov">
              <span className="font-mono">COV</span>
              <span className="ml-2 text-muted-foreground">Change of value</span>
            </SelectItem>
            <SelectItem value="interval">
              <span className="font-mono">Interval</span>
              <span className="ml-2 text-muted-foreground">Fixed period</span>
            </SelectItem>
            <SelectItem value="on_demand">
              <span className="font-mono">On demand</span>
              <span className="ml-2 text-muted-foreground">Manual record only</span>
            </SelectItem>
          </SelectContent>
        </Select>
      </div>

      {/* Policy-specific fields */}
      {policyKind === "cov" && (
        <CovForm value={covFields} onChange={setCovFields} />
      )}
      {policyKind === "interval" && (
        <IntervalForm value={intervalFields} onChange={setIntervalFields} />
      )}
      {policyKind === "on_demand" && (
        <p className="rounded-md border border-border bg-muted/40 p-3 text-xs text-muted-foreground">
          Records are only written when you click <strong>Record now</strong> or
          call the API explicitly.
        </p>
      )}

      {/* Per-path split */}
      <PathsEditor value={paths} onChange={setPaths} suggestions={pathSuggestions} />

      {/* Error */}
      {(saveMutation.error ?? removeMutation.error) && (
        <p className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {errorMessage(saveMutation.error ?? removeMutation.error)}
        </p>
      )}

      {/* Actions */}
      <div className={cn("flex items-center gap-2", isConfigured && "justify-between")}>
        {isConfigured && (
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            disabled={isBusy}
            onClick={() => removeMutation.mutate()}
          >
            Remove
          </Button>
        )}
        <Button
          size="sm"
          disabled={isBusy || configQuery.isLoading}
          onClick={() => saveMutation.mutate()}
          className="ml-auto"
        >
          {saveMutation.isPending ? "Saving…" : isConfigured ? "Update" : "Enable recording"}
        </Button>
      </div>

      {saveMutation.isSuccess && (
        <p className="text-xs text-green-600 dark:text-green-400">
          Policy saved.
        </p>
      )}
    </div>
  );
}
