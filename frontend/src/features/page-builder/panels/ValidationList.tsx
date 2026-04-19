// Footer strip that lists every current validation issue. Reads
// `store.issues` only — no parsing, no fetching. Empty state is
// deliberately quiet so it disappears when the buffer is clean.

import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { useBuilderStore } from "../store/builder-store";

export function ValidationList() {
  const issues = useBuilderStore((s) => s.issues);

  if (issues.length === 0) {
    return (
      <div className="flex items-center gap-2 border-t border-border px-4 py-1.5 text-xs text-muted-foreground">
        <CheckCircle2 size={12} className="text-emerald-500" />
        No issues
      </div>
    );
  }

  return (
    <ul className="max-h-40 overflow-auto border-t border-border">
      {issues.map((issue, i) => (
        <li
          key={`${issue.location}:${i}`}
          className="flex items-start gap-2 px-4 py-1.5 text-xs"
        >
          <AlertTriangle
            size={12}
            className="mt-0.5 shrink-0 text-destructive"
          />
          <span className="font-mono text-muted-foreground">{issue.location}</span>
          <span className="min-w-0 flex-1 truncate" title={issue.message}>
            {issue.message}
          </span>
        </li>
      ))}
    </ul>
  );
}
