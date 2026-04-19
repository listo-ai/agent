// Debounced validator that publishes issues into the store.
//
// Lives in `persistence/` because it owns side effects — timers today,
// network-backed dry-run calls in a later pass. UI components read
// `issues` straight from the store; they never see the validator.

import { useEffect } from "react";
import { useBuilderStore } from "../store/builder-store";
import { validateLayout } from "../model/validate-layout";

const DEBOUNCE_MS = 300;

export function useValidator(): void {
  const layoutText = useBuilderStore((s) => s.draft?.layoutText);
  const setIssues = useBuilderStore((s) => s.setIssues);

  useEffect(() => {
    if (layoutText === undefined) return;
    const handle = window.setTimeout(() => {
      const result = validateLayout(layoutText);
      setIssues(result.ok ? [] : result.issues);
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [layoutText, setIssues]);
}
