import { Clipboard, Clock, FolderOpen, Hash, PlusCircle, Settings, Trash2 } from "lucide-react";
import type { NodeContextMenuItem } from "./types";

export interface BuildNodeContextItemsOptions {
  /** Navigate into this node so its children are shown on the canvas. */
  onOpen?: () => void;
  /** Open the Add-child-node dialog for this node. */
  onAddChild?: () => void;
  /** Open the slot-history panel for this node. */
  onHistory?: () => void;
  onSettings: () => void;
  onDelete: () => void;
}

export interface NodeCopyInfo {
  /** Full path of the node, e.g. "/flows/my-flow/my-node" */
  path: string;
  /** Node kind identifier, e.g. "acme.core.timer" */
  kindId?: string;
}

/**
 * Returns clipboard-copy items for the common node identifiers.
 * The first item gets a separator so these appear as a distinct group.
 */
export function buildCopyItems(info: NodeCopyInfo): NodeContextMenuItem[] {
  const copy = (text: string) => () => navigator.clipboard.writeText(text);

  const segments = info.path.replace(/\/$/, "").split("/");
  const nodeId = segments[segments.length - 1] ?? info.path;
  const parentId = segments.length >= 2 ? (segments[segments.length - 2] ?? "/") : "/";
  const parentPath = segments.slice(0, -1).join("/") || "/";

  const items: NodeContextMenuItem[] = [
    {
      label: `Copy node id`,
      icon: <Hash size={14} />,
      onClick: copy(nodeId),
      separator: true,
    },
    {
      label: `Copy parent id`,
      icon: <Hash size={14} />,
      onClick: copy(parentId),
    },
    {
      label: `Copy path`,
      icon: <Clipboard size={14} />,
      onClick: copy(info.path),
    },
    {
      label: `Copy parent path`,
      icon: <Clipboard size={14} />,
      onClick: copy(parentPath),
    },
  ];

  if (info.kindId) {
    items.push({
      label: `Copy kind`,
      icon: <Clipboard size={14} />,
      onClick: copy(info.kindId),
    });
  }

  return items;
}

/** Convenience factory for the standard node context-menu items. */
export function buildNodeContextItems({
  onOpen,
  onAddChild,
  onHistory,
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
  if (onHistory) {
    items.push({ label: "History", icon: <Clock size={14} />, onClick: onHistory });
  }
  items.push({ label: "Delete", icon: <Trash2 size={14} />, onClick: onDelete, variant: "destructive" });
  return items;
}
