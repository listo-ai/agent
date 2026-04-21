import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import type { UiResolveResponse } from "@listo/agent-client";
import type { ButtonNode } from "../types";
import { useSdui } from "../context";
import { useActionResponse } from "../useActionResponse";
import { mergeAt } from "../applyPatch";

export function ButtonComponent({ node }: { node: ButtonNode }) {
  const { dispatchAction, treeQueryKey } = useSdui();
  const qc = useQueryClient();
  const [loading, setLoading] = useState(false);
  const handleResponse = useActionResponse();

  async function handleClick() {
    if (!node.action || loading) return;

    // Optimistic hint — apply before the round-trip so the UI feels
    // instant. Server response authoritatively replaces via
    // Patch / FullRender (see useActionResponse).
    const hint = node.action.optimistic;
    let rollback: UiResolveResponse | undefined;
    if (hint && typeof hint.fields === "object" && hint.fields !== null) {
      rollback = qc.getQueryData<UiResolveResponse>(treeQueryKey);
      qc.setQueryData<UiResolveResponse>(treeQueryKey, (prev) => {
        if (!prev || !("render" in prev)) return prev;
        return {
          ...prev,
          render: mergeAt(
            prev.render,
            hint.target_component_id,
            hint.fields as Record<string, unknown>,
          ),
        };
      });
    }

    setLoading(true);
    try {
      const resp = await dispatchAction(node.action.handler, node.action.args);
      handleResponse(resp);
    } catch (err) {
      // Round-trip failed — restore the pre-optimistic state so the
      // user isn't left with a stale fake.
      if (rollback) qc.setQueryData(treeQueryKey, rollback);
      // eslint-disable-next-line no-console
      console.error("[sdui action]", err);
    } finally {
      setLoading(false);
    }
  }

  const variant =
    node.intent === "danger" ? "destructive"
    : node.intent === "muted" ? "secondary"
    : "default";

  return (
    <Button
      variant={variant}
      disabled={node.disabled ?? loading ?? !node.action}
      onClick={() => void handleClick()}
    >
      {loading ? "…" : node.label}
    </Button>
  );
}
