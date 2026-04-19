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

interface BuilderState {
  /** `null` until the initial node fetch completes. */
  draft: DraftPage | null;
  /** Parse + dry-run issues, newest dry-run wins. */
  issues: ValidationIssue[];

  hydrate(draft: DraftPage): void;
  setLayoutText(text: string): void;
  /** Replace the issues list — callers compose parse + dry-run sources. */
  setIssues(issues: ValidationIssue[]): void;
  reset(): void;
}

export const useBuilderStore = create<BuilderState>((set) => ({
  draft: null,
  issues: [],

  hydrate: (draft) => set({ draft, issues: [] }),
  setLayoutText: (text) =>
    set((s) => (s.draft ? { draft: { ...s.draft, layoutText: text } } : s)),
  setIssues: (issues) => set({ issues }),
  reset: () => set({ draft: null, issues: [] }),
}));
