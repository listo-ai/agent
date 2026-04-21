import { useState } from "react";
import { X, RefreshCw } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import type { NodeSnapshot } from "@listo/agent-client";

import { SmartSlotHistoryPanel } from "@/components/slot-history/SmartSlotHistoryPanel";
import { RecordHistoryButton } from "@/components/slot-history/RecordHistoryButton";
import { SlotHistoryConfigPanel } from "@/components/slot-history/SlotHistoryConfigPanel";
import { ScrollArea } from "@listo/ui-kit";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@listo/ui-kit";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@listo/ui-kit";
import { cn } from "@/lib/utils";
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
  const qc = useQueryClient();
  const [refreshing, setRefreshing] = useState(false);

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
        <div className="ml-3 flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={async () => {
              setRefreshing(true);
              await qc.invalidateQueries({ queryKey: ["slot-history"] });
              await qc.invalidateQueries({ queryKey: ["slot-telemetry"] });
              setRefreshing(false);
            }}
            className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-label="Refresh history"
          >
            <RefreshCw size={14} className={cn(refreshing && "animate-spin")} />
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-label="Close history panel"
          >
            <X size={14} />
          </button>
        </div>
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

      {/* Tabs: View / Configure */}
      <Tabs defaultValue="view" className="flex min-h-0 flex-1 flex-col">
        <div className="border-b border-border px-5">
          <TabsList className="h-8 rounded-none bg-transparent p-0">
            <TabsTrigger
              value="view"
              className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
            >
              View
            </TabsTrigger>
            <TabsTrigger
              value="configure"
              className="rounded-none border-b-2 border-transparent px-3 py-1.5 text-xs data-[state=active]:border-primary data-[state=active]:bg-transparent"
            >
              Configure
            </TabsTrigger>
          </TabsList>
        </div>

        {/* View tab */}
        <TabsContent value="view" className="flex min-h-0 flex-1 flex-col mt-0">
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
          {validSlot && (
            <div className="border-t border-border px-5 py-3">
              <RecordHistoryButton
                nodePath={node.path}
                slot={validSlot}
                className="w-full"
              />
            </div>
          )}
        </TabsContent>

        {/* Configure tab */}
        <TabsContent value="configure" className="mt-0 flex-1 overflow-auto px-5 py-4">
          {validSlot ? (
            <SlotHistoryConfigPanel
              nodePath={node.path}
              slot={validSlot}
            />
          ) : (
            <p className="py-8 text-center text-sm text-muted-foreground">
              Select a slot above to configure auto-recording.
            </p>
          )}
        </TabsContent>
      </Tabs>
    </aside>
  );
}
