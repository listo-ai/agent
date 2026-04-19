// Editor pane. PR 1 ships a read-only pretty-printed view of the
// draft — enough to prove the shell is wired end-to-end. PR 2 swaps
// the body for Monaco + `setLayoutText` on change.

import { useBuilderStore } from "../store/builder-store.js";

export function EditorPane() {
  const draft = useBuilderStore((s) => s.draft);
  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }
  return (
    <pre className="h-full overflow-auto bg-muted/20 p-4 font-mono text-xs leading-relaxed">
      {pretty(draft.layoutText)}
    </pre>
  );
}

function pretty(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}
