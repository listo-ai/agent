import type { ForbiddenNode, DanglingNode } from "../types";

export function ForbiddenComponent({ node }: { node: ForbiddenNode }) {
  return (
    <div className="flex items-center gap-2 rounded border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive/80">
      <span>🚫</span>
      <span>Not allowed{node.reason ? `: ${node.reason}` : ""}</span>
    </div>
  );
}

export function DanglingComponent({ node }: { node: DanglingNode }) {
  return (
    <div className="flex items-center gap-2 rounded border border-dashed border-muted-foreground/40 px-3 py-2 text-sm text-muted-foreground">
      <span>⚠</span>
      <span>Missing component {node.id}</span>
    </div>
  );
}
