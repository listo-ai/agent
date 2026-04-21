import { Badge } from "@listo/ui-kit";
import type { BadgeNode } from "../types";
import { cn } from "@/lib/utils";

const intentVariant: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
  danger: "destructive",
  warn: "outline",
  ok: "default",
  muted: "secondary",
};

export function BadgeComponent({ node }: { node: BadgeNode }) {
  const variant = node.intent ? intentVariant[node.intent] ?? "secondary" : "secondary";
  return <Badge variant={variant}>{node.label}</Badge>;
}
