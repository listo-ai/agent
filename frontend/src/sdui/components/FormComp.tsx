/**
 * `form` component — renders a JSON Schema form using rjsf.
 * The `schema_ref` is expected to be a JSON Schema object (already
 * resolved by the binding engine).  For this minimal S4 version we just
 * display the bound values read-only and offer a Submit button.
 */
import { useState } from "react";
import { Button } from "@listo/ui-kit";
import type { FormNode } from "../types";
import { useSdui } from "../context";
import { useActionResponse } from "../useActionResponse";

export function FormComponent({ node }: { node: FormNode }) {
  const { dispatchAction } = useSdui();
  const [loading, setLoading] = useState(false);
  const handleResponse = useActionResponse();

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!node.submit || loading) return;
    setLoading(true);
    try {
      const resp = await dispatchAction(node.submit.handler, {
        ...(typeof node.submit.args === "object" && node.submit.args !== null ? node.submit.args : {}),
        bindings: node.bindings,
      });
      handleResponse(resp);
    } finally {
      setLoading(false);
    }
  }

  return (
    <form onSubmit={(e) => void handleSubmit(e)} className="flex flex-col gap-3 rounded border p-4">
      <div className="text-xs text-muted-foreground">
        Form: <code>{node.schema_ref}</code>
      </div>
      {node.bindings != null && (
        <pre className="rounded bg-muted p-2 text-xs">
          {JSON.stringify(node.bindings, null, 2)}
        </pre>
      )}
      {node.submit && (
        <Button type="submit" disabled={loading}>
          {loading ? "Submitting…" : "Submit"}
        </Button>
      )}
    </form>
  );
}
