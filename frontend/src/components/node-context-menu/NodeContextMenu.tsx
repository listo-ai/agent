import { Fragment, useEffect, useRef } from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";
import type { NodeContextMenuProps } from "./types";

/**
 * Reusable right-click context menu for a node.
 * Rendered into document.body via a portal so it escapes any overflow:hidden ancestors.
 * Closes on outside click or Escape.
 */
export function NodeContextMenu({
  x,
  y,
  nodeLabel,
  items,
  onClose,
}: NodeContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click or Escape.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    const onMouse = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("keydown", onKey);
    // Use mousedown so it fires before click bubbling closes things.
    document.addEventListener("mousedown", onMouse);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onMouse);
    };
  }, [onClose]);

  // Keep menu on screen.
  const menuW = 192;
  const menuH = 40 + items.length * 36;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const left = x + menuW > vw ? x - menuW : x;
  const top  = y + menuH > vh ? y - menuH : y;

  return createPortal(
    <div
      ref={ref}
      className="fixed z-[9999] min-w-48 overflow-hidden rounded-xl border border-border bg-popover shadow-xl"
      style={{ left, top }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {/* Header */}
      <div className="border-b border-border px-3 py-2">
        <p className="truncate text-xs font-semibold text-foreground">{nodeLabel}</p>
      </div>

      {/* Items */}
      <div className="p-1">
        {items.map((item, i) => (
          <Fragment key={i}>
            {item.separator && (
              <div className="my-1 border-t border-border" />
            )}
            <button
              type="button"
              disabled={item.disabled}
              onClick={() => { item.onClick(); onClose(); }}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-sm transition-colors",
                "disabled:pointer-events-none disabled:opacity-40",
                item.variant === "destructive"
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-accent",
              )}
            >
              {item.icon && (
                <span className="shrink-0 opacity-70">{item.icon}</span>
              )}
              {item.label}
            </button>
          </Fragment>
        ))}
      </div>
    </div>,
    document.body,
  );
}
