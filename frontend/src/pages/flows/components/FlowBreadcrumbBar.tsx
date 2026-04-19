import { ArrowLeft } from "lucide-react";

interface FlowBreadcrumbBarProps {
  openFlowPath: string | null;
  onBack: () => void;
}

export function FlowBreadcrumbBar({ openFlowPath, onBack }: FlowBreadcrumbBarProps) {
  return (
    <div className="flex items-center gap-2 border-b border-border bg-card/40 px-4 py-2">
      <button
        type="button"
        onClick={onBack}
        className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
      >
        <ArrowLeft size={12} />
        All flows
      </button>
      <span className="font-mono text-xs text-muted-foreground">{openFlowPath ?? "…"}</span>
    </div>
  );
}
