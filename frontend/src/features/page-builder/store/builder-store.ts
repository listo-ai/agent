// Headless state container for one page-builder session.
//
// Rules:
//   - No React imports, no DOM, no `fetch`. React reads via hooks that
//     live in this file; network I/O lives in `persistence/` (added in
//     PR 3) and only calls into these actions.
//   - One draft per session. Switching pages unmounts the builder and
//     mounts a fresh store — keeps the OCC story clean.
//
// PR 1 surface is intentionally tiny: `hydrate` and `setLayoutText`.
// Autosave, dry-run validation, and conflict handling land in PR 2/3
// by extending this store — not by side-channelling state through
// React components.

import { create } from "zustand";
import type { DraftPage, ValidationIssue } from "../model/types.js";

export type SaveState =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "saved"; at: number }
  | { kind: "error"; message: string };

export interface ConflictState {
  /** Generation the server reported when the last write 409'd. */
  currentGeneration: number;
}

interface BuilderState {
  /** `null` until the initial node fetch completes. */
  draft: DraftPage | null;
  /** Parse + dry-run issues, newest dry-run wins. */
  issues: ValidationIssue[];
  /** Autosave state surfaced to the header strip. */
  saveState: SaveState;
  /**
   * Non-`null` when a concurrent write made our baseGeneration stale.
   * Blocks further edits until the user reloads or exports.
   */
  conflict: ConflictState | null;

  hydrate(draft: DraftPage): void;
  setLayoutText(text: string): void;
  setIssues(issues: ValidationIssue[]): void;
  setSaveState(state: SaveState): void;
  /** Bump the generation after a successful write. */
  markSaved(generation: number): void;
  setConflict(state: ConflictState | null): void;
  reset(): void;
}

export const useBuilderStore = create<BuilderState>((set) => ({
  draft: null,
  issues: [],
  saveState: { kind: "idle" },
  conflict: null,

  hydrate: (draft) =>
    set({
      draft,
      issues: [],
      saveState: { kind: "idle" },
      conflict: null,
    }),
  setLayoutText: (text) =>
    set((s) => (s.draft ? { draft: { ...s.draft, layoutText: text } } : s)),
  setIssues: (issues) => set({ issues }),
  setSaveState: (saveState) => set({ saveState }),
  markSaved: (generation) =>
    set((s) =>
      s.draft
        ? {
            draft: { ...s.draft, baseGeneration: generation },
            saveState: { kind: "saved", at: Date.now() },
          }
        : s,
    ),
  setConflict: (conflict) => set({ conflict }),
  reset: () =>
    set({
      draft: null,
      issues: [],
      saveState: { kind: "idle" },
      conflict: null,
    }),
}));
