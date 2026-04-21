/**
 * Apply a `UiActionResponse` to the owning resolve/render query.
 *
 * - `toast` / `navigate` / `download` / `none` — side effect, no tree
 *   mutation.
 * - `patch` / `full_render` — authoritative tree mutation. Written to
 *   the React-Query cache via `setQueryData`; subsequent renders
 *   project the new tree without a refetch.
 * - `form_errors` / `stream` — logged for now; richer routing when the
 *   `form` component owns error display.
 */
import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import type { UiActionResponse, UiResolveResponse } from "@listo/agent-client";
import { useSdui } from "./context";
import { replaceAt } from "./applyPatch";

export function useActionResponse() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { treeQueryKey } = useSdui();

  return useCallback(
    (resp: UiActionResponse) => {
      switch (resp.type) {
        case "none":
          return;
        case "toast":
          // eslint-disable-next-line no-console
          console.info(`[sdui toast] ${resp.intent}: ${resp.message}`);
          window.alert(`${resp.intent.toUpperCase()}: ${resp.message}`);
          return;
        case "navigate":
          if (resp.to.target_ref) {
            void navigate(`/ui/${encodeURIComponent(resp.to.target_ref)}`);
          }
          return;
        case "full_render":
          qc.setQueryData<UiResolveResponse>(treeQueryKey, (prev) => {
            if (!prev || !("render" in prev)) return prev;
            return { ...prev, render: resp.tree };
          });
          return;
        case "patch":
          qc.setQueryData<UiResolveResponse>(treeQueryKey, (prev) => {
            if (!prev || !("render" in prev)) return prev;
            return {
              ...prev,
              render: replaceAt(
                prev.render,
                resp.target_component_id,
                resp.tree.root,
              ),
            };
          });
          return;
        default:
          // form_errors, download, stream — fall through for richer
          // routing later.
          // eslint-disable-next-line no-console
          console.debug("[sdui action response]", resp);
      }
    },
    [navigate, qc, treeQueryKey],
  );
}
