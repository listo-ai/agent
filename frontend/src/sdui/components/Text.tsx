import type { TextNode } from "../types";
import { cn } from "@/lib/utils";

const intentClass: Record<string, string> = {
  danger: "text-destructive",
  warn: "text-amber-500",
  ok: "text-green-600",
  muted: "text-muted-foreground",
};

export function TextComponent({ node }: { node: TextNode }) {
  return (
    <p className={cn("text-sm", node.intent ? intentClass[node.intent] : undefined)}>
      {node.content}
    </p>
  );
}
