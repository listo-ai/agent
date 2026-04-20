/**
 * FlowNodeCard — the card rendered for each node on the flow canvas.
 *
 * Reads from `LiveDataContext` (slot values) and `PresentationStore`
 * (runtime status / color / icon / message) so status ticks don't
 * re-render the canvas structure.
 */
import * as React from "react";
import { memo, useContext } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import * as LucideIcons from "lucide-react";
import { cn } from "@/lib/utils";
import { formatLiveValue, mergedSlots, titleForKind, type FlowNodeData } from "../flow-model";
import { LiveDataContext } from "./FlowCanvasContext";
import { useNodePresentation, type NodeStatus } from "@/store/presentation-store";

// ---------------------------------------------------------------------------
// Layout constants — shared with handle positioning in FlowCanvas.tsx
// ---------------------------------------------------------------------------
export const CARD_PADDING_TOP = 12; // pt-3
export const HEADER_HEIGHT = 52;    // title line + kind line + gaps
export const SLOT_ROW_H = 22;       // height per slot row
export const SLOTS_PADDING_TOP = 6; // gap before slot rows

// ---------------------------------------------------------------------------
// Status dot
// ---------------------------------------------------------------------------

const STATUS_DOT_CLASS: Record<NodeStatus, string | null> = {
  None: null,
  Unknown: "bg-slate-400",
  Ok: "bg-emerald-500",
  Warning: "bg-amber-400",
  Error: "bg-red-500",
};

function StatusDot({ status, message }: { status: NodeStatus; message?: string | undefined }) {
  const cls = STATUS_DOT_CLASS[status];
  if (!cls) return null;
  return (
    <span
      title={message}
      className={cn("mt-1.5 h-2.5 w-2.5 shrink-0 rounded-full", cls)}
      aria-label={`Status: ${status}`}
    />
  );
}

// ---------------------------------------------------------------------------
// Dynamic icon from lucide name
// ---------------------------------------------------------------------------

function resolveIcon(name: string): React.ComponentType<{ className?: string | undefined }> | null {
  const pascal = name.replace(/(^|[-_])([a-z])/g, (_, _sep, c: string) => c.toUpperCase());
  const icon = (LucideIcons as Record<string, unknown>)[pascal];
  if (typeof icon === "function") return icon as React.ComponentType<{ className?: string | undefined }>;
  return null;
}

function NodeIcon({ name, className }: { name: string; className?: string | undefined }) {
  const Icon = resolveIcon(name);
  if (!Icon) return null;
  return <Icon className={className} />;
}

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

export const FlowNodeCard = memo(function FlowNodeCard({
  data,
  selected,
}: NodeProps<Node<FlowNodeData>>) {
  const liveByPath = useContext(LiveDataContext);
  const live = liveByPath[data.snapshot.path];
  const slots = mergedSlots(data.snapshot, live);
  const presentation = useNodePresentation(data.snapshot.id);

  const inputs = data.kind?.slots.filter((slot) => slot.role === "input") ?? [];
  const outputs = data.kind?.slots.filter((slot) => slot.role === "output") ?? [];
  const statusSlots = slots.filter((slot) =>
    (data.kind?.slots.find((entry) => entry.name === slot.name)?.role ?? "status") === "status",
  );
  const slotRows = Math.max(inputs.length, outputs.length);
  const slotsBlockTop = CARD_PADDING_TOP + HEADER_HEIGHT + SLOTS_PADDING_TOP;

  // Accent color: presentation override or undefined (theme default).
  const accentStyle = presentation.color
    ? { borderTopColor: presentation.color, borderTopWidth: 3 }
    : undefined;

  // Icon: presentation override only (Kind schema carries no static icon field).
  const iconName = presentation.icon;

  return (
    <div
      className={cn(
        "relative rounded-2xl border bg-background/95 shadow-sm transition-shadow",
        selected ? "border-primary shadow-md" : "border-border",
      )}
      style={{ width: 240, ...accentStyle }}
    >
      {/* Handles */}
      {inputs.map((slot, index) => (
        <Handle
          key={`in-${slot.name}`}
          id={slot.name}
          type="target"
          position={Position.Left}
          style={{ top: slotsBlockTop + index * SLOT_ROW_H + SLOT_ROW_H / 2 }}
          className="!h-3 !w-3 !border-2 !border-background !bg-slate-500"
        />
      ))}
      {outputs.map((slot, index) => (
        <Handle
          key={`out-${slot.name}`}
          id={slot.name}
          type="source"
          position={Position.Right}
          style={{ top: slotsBlockTop + index * SLOT_ROW_H + SLOT_ROW_H / 2 }}
          className="!h-3 !w-3 !border-2 !border-background !bg-blue-500"
        />
      ))}

      {/* Header */}
      <div className="flex items-start justify-between gap-2 px-4 pt-3 pb-1">
        <div className="flex min-w-0 flex-1 items-start gap-2">
          {iconName && (
            <NodeIcon
              name={iconName}
              className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
            />
          )}
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold leading-snug">
              {titleForKind(data.kind, data.snapshot.path.split("/").pop())}
            </div>
            <div className="truncate text-[11px] text-muted-foreground">
              {data.snapshot.kind}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <StatusDot status={presentation.status} message={presentation.message} />
          <span className="mt-0.5 shrink-0 rounded-full bg-secondary px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-secondary-foreground">
            {live?.lifecycle ?? data.snapshot.lifecycle}
          </span>
        </div>
      </div>

      {/* Slot labels row */}
      {slotRows > 0 && (
        <div
          className="relative mx-0 border-t border-border/40"
          style={{ height: slotRows * SLOT_ROW_H + SLOTS_PADDING_TOP * 2 }}
        >
          {inputs.map((slot, index) => (
            <span
              key={`label-in-${slot.name}`}
              className="pointer-events-none absolute left-5 text-[10px] font-medium uppercase tracking-wide text-slate-500"
              style={{ top: SLOTS_PADDING_TOP + index * SLOT_ROW_H + SLOT_ROW_H / 2, transform: "translateY(-50%)" }}
            >
              {slot.name}
            </span>
          ))}
          {outputs.map((slot, index) => (
            <span
              key={`label-out-${slot.name}`}
              className="pointer-events-none absolute right-5 text-[10px] font-medium uppercase tracking-wide text-blue-600"
              style={{ top: SLOTS_PADDING_TOP + index * SLOT_ROW_H + SLOT_ROW_H / 2, transform: "translateY(-50%)" }}
            >
              {slot.name}
            </span>
          ))}
        </div>
      )}

      {/* Status chips */}
      {statusSlots.length > 0 && (
        <div className="flex flex-wrap gap-2 border-t border-border/40 px-4 py-2">
          {statusSlots.slice(0, 4).map((slot) => (
            <span
              key={slot.name}
              className="rounded-full bg-slate-100 px-2 py-0.5 text-[11px] font-medium text-slate-700"
            >
              {slot.name}: {formatLiveValue(slot.value)}
            </span>
          ))}
        </div>
      )}

      {/* Footer */}
      <div className="flex items-center justify-between border-t border-border/40 px-4 py-2 text-[11px] text-muted-foreground">
        <span className="truncate">{data.snapshot.path}</span>
        <span className="ml-2 shrink-0">{inputs.length} in · {outputs.length} out</span>
      </div>
    </div>
  );
});
