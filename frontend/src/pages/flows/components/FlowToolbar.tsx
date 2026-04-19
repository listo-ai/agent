interface FlowToolbarProps {
  selectedFlowPath: string | null;
  nodeCount: number;
  onAutoLayout: () => void;
  onFitView: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onToggleMiniMap: () => void;
  onToggleGrid: () => void;
  showMiniMap: boolean;
  showGrid: boolean;
}

export function FlowToolbar({
  selectedFlowPath,
  nodeCount,
  onAutoLayout,
  onFitView,
  onZoomIn,
  onZoomOut,
  onToggleMiniMap,
  onToggleGrid,
  showMiniMap,
  showGrid,
}: FlowToolbarProps) {
  return (
    <header className="flex items-center justify-between border-b border-border bg-background/95 px-4 py-3 backdrop-blur">
      <div>
        <h2 className="text-sm font-semibold">{selectedFlowPath ?? "No flow selected"}</h2>
        <p className="text-xs text-muted-foreground">
          {selectedFlowPath ? `${nodeCount} node${nodeCount === 1 ? "" : "s"} on canvas` : "Pick or create a flow to begin."}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <ToolbarButton onClick={onAutoLayout} disabled={!selectedFlowPath || nodeCount === 0}>
          Auto-layout
        </ToolbarButton>
        <ToolbarButton onClick={onFitView} disabled={!selectedFlowPath || nodeCount === 0}>
          Fit
        </ToolbarButton>
        <ToolbarButton onClick={onZoomOut} disabled={!selectedFlowPath || nodeCount === 0}>
          -
        </ToolbarButton>
        <ToolbarButton onClick={onZoomIn} disabled={!selectedFlowPath || nodeCount === 0}>
          +
        </ToolbarButton>
        <ToolbarButton onClick={onToggleMiniMap}>{showMiniMap ? "Hide minimap" : "Show minimap"}</ToolbarButton>
        <ToolbarButton onClick={onToggleGrid}>{showGrid ? "Hide grid" : "Show grid"}</ToolbarButton>
      </div>
    </header>
  );
}

function ToolbarButton({
  children,
  disabled,
  onClick,
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded-md border border-border px-3 py-1.5 text-xs font-medium transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
    >
      {children}
    </button>
  );
}
