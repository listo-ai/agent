// Debounced validator that publishes issues into the store.
//
// Two passes per tick:
//   1. Local parse + shape check (`validateLayout`). Fast, no network.
//      Parse failures short-circuit — server dry-run is pointless on
//      unparseable JSON.
//   2. Server `/ui/resolve --dry-run` with the parsed candidate as
//      `layout` override. Evaluates bindings against live graph state
//      and flags `$target.*`, unresolved `$page.*`, and slot-walk
//      failures. Requires the inline-layout override added alongside.
//
// The hook owns the AbortController for the in-flight dry-run so a
// burst of keystrokes never stacks up stale validations.

import { useEffect, useRef } from "react";
import { agentPromise } from "@listo/ui-core";
import { useBuilderStore } from "../store/builder-store";
import { validateLayout } from "../model/validate-layout";
import type { ValidationIssue } from "../model/types";

const DEBOUNCE_MS = 300;

export function useValidator(): void {
  const layoutText = useBuilderStore((s) => s.draft?.layoutText);
  const nodeId = useBuilderStore((s) => s.draft?.nodeId);
  const setIssues = useBuilderStore((s) => s.setIssues);
  const generation = useRef(0);

  useEffect(() => {
    if (layoutText === undefined || !nodeId) return;
    const tick = ++generation.current;
    const handle = window.setTimeout(async () => {
      const local = validateLayout(layoutText);
      if (!local.ok) {
        if (tick === generation.current) setIssues(local.issues);
        return;
      }
      // Local parse clean — ask the server to resolve bindings.
      try {
        const client = await agentPromise;
        const resp = await client.ui.resolve({
          page_ref: nodeId,
          stack: [],
          page_state: {},
          dry_run: true,
          user_claims: {},
          layout: local.value,
        });
        if (tick !== generation.current) return;
        const serverIssues: ValidationIssue[] =
          "errors" in resp
            ? resp.errors.map((e) => ({
                source: "dry-run",
                location: e.location,
                message: e.message,
              }))
            : [];
        setIssues(serverIssues);
      } catch (err) {
        if (tick !== generation.current) return;
        setIssues([
          {
            source: "dry-run",
            location: "root",
            message:
              err instanceof Error
                ? `dry-run failed: ${err.message}`
                : "dry-run failed",
          },
        ]);
      }
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(handle);
  }, [layoutText, nodeId, setIssues]);
}
