/**
 * NodeAppearanceDialog — right-click "Tags & appearance" dialog.
 *
 * Combines TagsEditor and AppearanceEditor in one focused dialog.
 * Both editors auto-save with debounce; no explicit Save button needed.
 */
import type { Slot } from "@listo/agent-client";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@listo/ui-kit";
import { useSlotWriter } from "@/lib/slots";
import { TagsEditor, useNodeTags } from "@/lib/node/tags";
import { AppearanceEditor, useNodeAppearance } from "@/lib/node/appearance";

interface NodeAppearanceDialogProps {
  open: boolean;
  onClose: () => void;
  /** Display name shown in the dialog title. */
  nodeLabel: string;
  nodePath: string | undefined;
  /** Live slot map for the node — provides `config.tags` and `config.appearance`. */
  live: Record<string, Slot>;
}

export function NodeAppearanceDialog({
  open,
  onClose,
  nodeLabel,
  nodePath,
  live,
}: NodeAppearanceDialogProps) {
  const saveTags = useSlotWriter("config.tags");
  const saveAppearance = useSlotWriter("config.appearance");
  const tagsState = useNodeTags(nodePath, live["config.tags"], saveTags);
  const appearanceState = useNodeAppearance(
    nodePath,
    live["config.appearance"],
    saveAppearance,
  );

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose(); }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="truncate text-sm">
            Tags &amp; appearance — {nodeLabel}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-6 pt-1">
          {/* Tags */}
          <section>
            <h3 className="mb-2.5 text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Tags
            </h3>
            <TagsEditor {...tagsState} />
          </section>

          {/* Appearance */}
          <section>
            <h3 className="mb-2.5 text-xs font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              Appearance
            </h3>
            <AppearanceEditor {...appearanceState} />
          </section>
        </div>
      </DialogContent>
    </Dialog>
  );
}
