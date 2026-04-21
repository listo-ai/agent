// Debounced OCC-guarded save for the `layout` slot.
//
// Contract:
//   - Saves only when local parse succeeds. Binding-level (server
//     dry-run) issues do not block save — a half-authored page with a
//     dangling `$target` is still a save-worthy draft.
//   - Never runs while a conflict is pending. The banner takes over.
//   - Every successful save advances `baseGeneration`, so subsequent
//     OCC guards compare against the latest server generation.
//   - A 409 surfaces as `GenerationMismatchError` from the TS client;
//     we record the server's `currentGeneration` and stop writing.

import { useEffect, useRef } from "react";
import { agentPromise } from "@/lib/agent";
import { GenerationMismatchError } from "@listo/agent-client";
import { useBuilderStore } from "../store/builder-store";
import { validateLayout } from "../model/validate-layout";

const DEBOUNCE_MS = 800;
const LAYOUT_SLOT = "layout";

export function useAutosave(): void {
  const draft = useBuilderStore((s) => s.draft);
  const conflict = useBuilderStore((s) => s.conflict);
  const setSaveState = useBuilderStore((s) => s.setSaveState);
  const markSaved = useBuilderStore((s) => s.markSaved);
  const setConflict = useBuilderStore((s) => s.setConflict);

  // Remember the last text we actually sent to the server so we
  // don't re-save on no-op ticks (e.g. cursor moves that re-render).
  const lastSent = useRef<string | null>(null);

  // Save as long as the text parses locally. Dry-run (binding /
  // schema) issues are surfaced in the validation strip but never
  // block persistence — a half-authored page with a dangling
  // `$target` or a typo'd `$page.key` is still a save-worthy draft,
  // and forcing the user to make it resolve-clean mid-edit broke the
  // authoring flow.

  useEffect(() => {
    if (!draft || conflict) return;
    if (draft.layoutText === lastSent.current) return;

    const snapshot = {
      path: draft.nodePath,
      text: draft.layoutText,
      baseGeneration: draft.baseGeneration,
    };
    const handle = window.setTimeout(async () => {
      const parsed = validateLayout(snapshot.text);
      if (!parsed.ok) return; // wait for clean parse
      lastSent.current = snapshot.text;
      setSaveState({ kind: "saving" });
      try {
        const client = await agentPromise;
        const newGen = await client.slots.writeSlot(
          snapshot.path,
          LAYOUT_SLOT,
          parsed.value,
          { expectedGeneration: snapshot.baseGeneration },
        );
        markSaved(newGen);
      } catch (err) {
        if (err instanceof GenerationMismatchError) {
          lastSent.current = null; // user may retry after reload
          setConflict({ currentGeneration: err.currentGeneration });
          setSaveState({ kind: "idle" });
          return;
        }
        const message = err instanceof Error ? err.message : String(err);
        setSaveState({ kind: "error", message });
      }
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [draft, conflict, setSaveState, markSaved, setConflict]);
}
