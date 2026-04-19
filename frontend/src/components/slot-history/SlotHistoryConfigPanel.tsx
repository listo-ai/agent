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

interface CovFields {
  policy: "cov";
  deadband: number;
  min_interval_ms: number;
  max_gap_ms: number;
  max_samples?: number | undefined;
}

interface IntervalFields {
  policy: "interval";
  period_ms: number;
  align_to_wall: boolean;
  max_samples?: number | undefined;
}

interface OnDemandFields {
  policy: "on_demand";
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

  // Sync form when query resolves
  useEffect(() => {
    if (!existingPolicy) return;
    setPolicyKind(existingPolicy.policy as PolicyKind);
    if (existingPolicy.policy === "cov") setCovFields(existingPolicy);
    if (existingPolicy.policy === "interval") setIntervalFields(existingPolicy);
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
