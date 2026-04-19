import { useState } from "react";
import { X } from "lucide-react";
import type { NodeSnapshot } from "@sys/agent-client";

import { SmartSlotHistoryPanel } from "@/components/slot-history/SmartSlotHistoryPanel";
import { RecordHistoryButton } from "@/components/slot-history/RecordHistoryButton";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { mergedSlots } from "../flow-model";

/** Slots managed by the canvas that aren't useful to track history for. */
const HIDE_SLOTS = new Set(["position"]);

interface NodeHistoryPanelProps {
  node: NodeSnapshot | undefined;
  onClose: () => void;
}

/**
 * Right-side panel for browsing and recording slot history on a node.
 * Drop it in place of (or alongside) FlowPropertyPanel in the page layout.
 *
 * Layout mirrors FlowPropertyPanel: fixed aside width, scrollable body.
 */
export function NodeHistoryPanel({ node, onClose }: NodeHistoryPanelProps) {
  const slots = node
    ? mergedSlots(node).filter((s) => !HIDE_SLOTS.has(s.name))
    : [];

  const [selectedSlot, setSelectedSlot] = useState<string>(
    slots[0]?.name ?? "",
  );

  // Keep selectedSlot valid when node changes.
  const validSlot =
    slots.some((s) => s.name === selectedSlot) ? selectedSlot : (slots[0]?.name ?? "");

  const currentSlotDef = slots.find((s) => s.name === validSlot);

  if (!node) {
    return (
      <aside className="flex h-full w-96 shrink-0 items-center justify-center border-l border-border bg-card/70 p-6">
        <p className="text-sm text-muted-foreground">No node selected.</p>
      </aside>
    );
  }

  return (
    <aside className="flex h-full w-96 shrink-0 flex-col border-l border-border bg-card/70">
      {/* Header */}
      <div className="flex items-start justify-between border-b border-border px-5 py-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold">
            {node.path.split("/").pop()}
          </h2>
          <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
            {node.path}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">Slot history</p>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="ml-3 shrink-0 rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label="Close history panel"
        >
          <X size={14} />
        </button>
      </div>

      {/* Slot selector */}
      <div className="border-b border-border px-5 py-3">
        <label className="mb-1.5 block text-xs font-medium text-muted-foreground">
          Slot
        </label>
        {slots.length === 0 ? (
          <p className="text-sm text-muted-foreground">No slots on this node.</p>
        ) : (
          <Select value={validSlot} onValueChange={setSelectedSlot}>
            <SelectTrigger size="sm" className="w-full">
              <SelectValue placeholder="Select a slot" />
            </SelectTrigger>
            <SelectContent>
              {slots.map((s) => (
                <SelectItem key={s.name} value={s.name}>
                  <span className="font-mono">{s.name}</span>
                  {s.value !== null && s.value !== undefined && (
                    <span className="ml-2 text-muted-foreground">
                      ({typeof s.value})
                    </span>
                  )}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      {/* History data */}
      <ScrollArea className="min-h-0 flex-1 px-5 py-4">
        {validSlot ? (
          <SmartSlotHistoryPanel
            nodePath={node.path}
            slot={validSlot}
            currentValue={currentSlotDef?.value}
          />
        ) : (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Select a slot above to view its history.
          </p>
        )}
      </ScrollArea>

      {/* Footer: record button */}
      {validSlot && (
        <div className="border-t border-border px-5 py-3">
          <RecordHistoryButton
            nodePath={node.path}
            slot={validSlot}
            className="w-full"
          />
        </div>
      )}
    </aside>
  );
}
