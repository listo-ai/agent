import { FolderOpen, PlusCircle, Settings, Trash2 } from "lucide-react";
import type { NodeContextMenuItem } from "./types";

export interface BuildNodeContextItemsOptions {
  /** Navigate into this node so its children are shown on the canvas. */
  onOpen?: () => void;
  /** Open the Add-child-node dialog for this node. */
  onAddChild?: () => void;
  onSettings: () => void;
  onDelete: () => void;
}

/** Convenience factory for the standard node context-menu items. */
export function buildNodeContextItems({
  onOpen,
  onAddChild,
  onSettings,
  onDelete,
}: BuildNodeContextItemsOptions): NodeContextMenuItem[] {
  const items: NodeContextMenuItem[] = [];
  if (onOpen) {
    items.push({ label: "Open", icon: <FolderOpen size={14} />, onClick: onOpen });
  }
  if (onAddChild) {
    items.push({ label: "Add child node", icon: <PlusCircle size={14} />, onClick: onAddChild });
  }
  items.push({ label: "Settings", icon: <Settings size={14} />, onClick: onSettings });
  items.push({ label: "Delete", icon: <Trash2 size={14} />, onClick: onDelete, variant: "destructive" });
  return items;
}
