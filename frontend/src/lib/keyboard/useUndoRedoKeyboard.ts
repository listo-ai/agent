/**
 * Binds Ctrl+Z (undo) and Ctrl+Y / Ctrl+Shift+Z (redo) to arbitrary callbacks.
 *
 * Generic — not tied to flows or any domain concept. Use it anywhere in the
 * app that needs keyboard undo/redo shortcut support.
 *
 * Skips when focus is inside an input, textarea, select, or contenteditable
 * so text-editing shortcuts still work normally.
 */
import { useEffect } from "react";

export interface UseUndoRedoKeyboardOptions {
  onUndo: () => void;
  onRedo: () => void;
  /** Set to false to temporarily disable the shortcuts (e.g. while a dialog is open). */
  enabled?: boolean;
}

function isEditableTarget(el: EventTarget | null): boolean {
  if (!el || !(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (el.isContentEditable) return true;
  return false;
}

export function useUndoRedoKeyboard({
  onUndo,
  onRedo,
  enabled = true,
}: UseUndoRedoKeyboardOptions): void {
  useEffect(() => {
    if (!enabled) return;

    const handler = (e: KeyboardEvent) => {
      if (isEditableTarget(e.target)) return;

      const isCtrl = e.ctrlKey || e.metaKey;
      if (!isCtrl) return;

      if (e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        onUndo();
      } else if (e.key === "y" || (e.key === "z" && e.shiftKey)) {
        e.preventDefault();
        onRedo();
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [enabled, onUndo, onRedo]);
}
