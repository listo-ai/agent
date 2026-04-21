import {
  Layout,
  Maximize,
  ZoomIn,
  ZoomOut,
  Map,
  Grid3x3,
  Undo2,
  Redo2,
} from "lucide-react";
import {
  Button,
  Separator,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui";

interface FlowToolbarProps {
  selectedFlowPath: string | null;
  nodeCount: number;
  onUndo: () => void;
  onRedo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  undoRedoPending: boolean;
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
  onUndo,
  onRedo,
  canUndo,
  canRedo,
  undoRedoPending,
  onAutoLayout,
  onFitView,
  onZoomIn,
  onZoomOut,
  onToggleMiniMap,
  onToggleGrid,
  showMiniMap,
  showGrid,
}: FlowToolbarProps) {
  const noNodes = !selectedFlowPath || nodeCount === 0;

  return (
    <header className="flex items-center justify-between border-b border-border bg-background/95 px-4 py-2 backdrop-blur">
      <div>
        <h2 className="text-sm font-semibold">{selectedFlowPath ?? "No flow selected"}</h2>
        <p className="text-xs text-muted-foreground">
          {selectedFlowPath ? `${nodeCount} node${nodeCount === 1 ? "" : "s"} on canvas` : "Pick or create a flow to begin."}
        </p>
      </div>
      <div className="flex items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon-xs"
              onClick={onUndo}
              disabled={!canUndo || undoRedoPending}
              aria-label="Undo (Ctrl+Z)"
            >
              <Undo2 size={13} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Undo (Ctrl+Z)</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon-xs"
              onClick={onRedo}
              disabled={!canRedo || undoRedoPending}
              aria-label="Redo (Ctrl+Y)"
            >
              <Redo2 size={13} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Redo (Ctrl+Y)</TooltipContent>
        </Tooltip>

        <Separator orientation="vertical" className="mx-1 h-4" />

        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="xs" onClick={onAutoLayout} disabled={noNodes}>
              <Layout size={13} /> Auto-layout
            </Button>
          </TooltipTrigger>
          <TooltipContent>Arrange nodes automatically</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="xs" onClick={onFitView} disabled={noNodes}>
              <Maximize size={13} /> Fit
            </Button>
          </TooltipTrigger>
          <TooltipContent>Fit all nodes in view</TooltipContent>
        </Tooltip>

        <Separator orientation="vertical" className="mx-1 h-4" />

        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="icon-xs" onClick={onZoomOut} disabled={noNodes}>
              <ZoomOut size={13} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Zoom out</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="outline" size="icon-xs" onClick={onZoomIn} disabled={noNodes}>
              <ZoomIn size={13} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Zoom in</TooltipContent>
        </Tooltip>

        <Separator orientation="vertical" className="mx-1 h-4" />

        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={showMiniMap ? "secondary" : "outline"}
              size="xs"
              onClick={onToggleMiniMap}
            >
              <Map size={13} /> Minimap
            </Button>
          </TooltipTrigger>
          <TooltipContent>{showMiniMap ? "Hide" : "Show"} minimap</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant={showGrid ? "secondary" : "outline"}
              size="xs"
              onClick={onToggleGrid}
            >
              <Grid3x3 size={13} /> Grid
            </Button>
          </TooltipTrigger>
          <TooltipContent>{showGrid ? "Hide" : "Show"} grid</TooltipContent>
        </Tooltip>
      </div>
    </header>
  );
}
