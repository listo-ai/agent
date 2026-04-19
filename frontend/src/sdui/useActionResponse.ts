/**
 * Processes `UiActionResponse` variants.  Full routing + patch application
 * is S4+; this minimal version covers `toast`, `none`, and `navigate`
 * (console log everything else so it shows up in devtools).
 */
import { useCallback } from "react";
import { useNavigate } from "react-router-dom";
import type { UiActionResponse } from "@sys/agent-client";

export function useActionResponse() {
  const navigate = useNavigate();

  return useCallback(
    (resp: UiActionResponse) => {
      switch (resp.type) {
        case "none":
          break;
        case "toast":
          // Simple fallback until a real toast provider is wired up.
          // eslint-disable-next-line no-console
          console.info(`[sdui toast] ${resp.intent}: ${resp.message}`);
          window.alert(`${resp.intent.toUpperCase()}: ${resp.message}`);
          break;
        case "navigate":
          if (resp.to.target_ref) void navigate(`/ui/${encodeURIComponent(resp.to.target_ref)}`);
          break;
        default:
          // patch, full_render, form_errors, download, stream
          // eslint-disable-next-line no-console
          console.debug("[sdui action response]", resp);
      }
    },
    [navigate],
  );
}
