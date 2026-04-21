import type { HistoryQueryOptions } from "@listo/agent-client";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { SlotHistoryPanel } from "./SlotHistoryPanel";
import { RecordHistoryButton } from "./RecordHistoryButton";

interface SlotHistoryDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Full node path, e.g. `/heartbeat`. */
  nodePath: string;
  /** Slot name, e.g. `notes`. */
  slot: string;
  opts?: HistoryQueryOptions;
}

/**
 * Dialog shell around `SlotHistoryPanel`. Use this from context menus,
 * settings pages, or anywhere a full-page panel would be too heavy.
 *
 * The caller controls open state — keep the open/close logic in the parent
 * so the dialog doesn't need to know about routing or menus.
 */
export function SlotHistoryDialog({
  open,
  onOpenChange,
  nodePath,
  slot,
  opts,
}: SlotHistoryDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="font-mono text-base">
            {nodePath}
            <span className="text-muted-foreground"> / </span>
            {slot}
            <span className="ml-2 text-sm font-normal text-muted-foreground">
              history
            </span>
          </DialogTitle>
        </DialogHeader>

        <ScrollArea className="max-h-[60vh]">
          <SlotHistoryPanel nodePath={nodePath} slot={slot} {...(opts !== undefined && { opts })} />
        </ScrollArea>

        <DialogFooter>
          <RecordHistoryButton nodePath={nodePath} slot={slot} />
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
