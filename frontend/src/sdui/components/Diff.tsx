import type { DiffNode } from "../types";

export function DiffComponent({ node }: { node: DiffNode }) {
  const oldLines = node.old_text.split("\n");
  const newLines = node.new_text.split("\n");
  return (
    <div className="grid grid-cols-2 gap-2 rounded border bg-muted/30 p-2 font-mono text-xs">
      <div>
        <p className="mb-1 text-muted-foreground">Before</p>
        {oldLines.map((l, i) => (
          <div key={i} className="text-destructive/80">{l || " "}</div>
        ))}
      </div>
      <div>
        <p className="mb-1 text-muted-foreground">After</p>
        {newLines.map((l, i) => (
          <div key={i} className="text-green-600">{l || " "}</div>
        ))}
      </div>
    </div>
  );
}
