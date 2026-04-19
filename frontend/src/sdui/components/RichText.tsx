import type { RichTextNode } from "../types";

export function RichTextComponent({ node }: { node: RichTextNode }) {
  // Minimum: display as preformatted text. Full editor (tiptap/milkdown) is S6.
  return (
    <div className="rounded border bg-muted/30 p-3">
      {node.value ? (
        <pre className="whitespace-pre-wrap text-sm">{node.value}</pre>
      ) : (
        <span className="text-sm text-muted-foreground">{node.placeholder ?? "Empty"}</span>
      )}
    </div>
  );
}
