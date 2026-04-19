export interface NodeContextMenuItem {
  label: string;
  icon?: React.ReactNode;
  onClick: () => void;
  variant?: "default" | "destructive";
  disabled?: boolean;
}

export interface NodeContextMenuProps {
  /** Viewport X position (from mouseEvent.clientX). */
  x: number;
  /** Viewport Y position (from mouseEvent.clientY). */
  y: number;
  /** Human-readable name shown at the top of the menu. */
  nodeLabel: string;
  items: NodeContextMenuItem[];
  onClose: () => void;
}
