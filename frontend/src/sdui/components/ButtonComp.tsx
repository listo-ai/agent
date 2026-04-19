import { useState } from "react";
import { Button } from "@/components/ui/button";
import type { ButtonNode } from "../types";
import { useSdui } from "../context";
import { useActionResponse } from "../useActionResponse";

export function ButtonComponent({ node }: { node: ButtonNode }) {
  const { dispatchAction } = useSdui();
  const [loading, setLoading] = useState(false);
  const handleResponse = useActionResponse();

  async function handleClick() {
    if (!node.action || loading) return;
    setLoading(true);
    try {
      const resp = await dispatchAction(node.action.handler, node.action.args);
      handleResponse(resp);
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
