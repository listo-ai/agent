import { GitBranch, Plus, Pencil, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  Badge,
  Button,
  Card,
  CardContent,
  Input,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@listo/ui-kit";

import { useFlowsList, type FlowRow } from "./useFlowsList";

// ---------------------------------------------------------------------------
// Page (thin shell — hooks for logic, components for presentation)
// ---------------------------------------------------------------------------

export function FlowsListPage() {
  const vm = useFlowsList();

  if (vm.status === "loading") return <LoadingSkeleton />;
  if (vm.status === "error")
    return (
      <CenteredMessage title="Could not reach the agent" detail={vm.errorDetail} />
    );

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      {/* Header bar */}
      <header className="flex items-center gap-3">
        <GitBranch size={18} className="text-primary" />
        <h1 className="text-base font-semibold">Flows</h1>
        <span className="text-xs text-muted-foreground">
          One row per <code>sys.core.flow</code> container.
        </span>
        <Input
          value={vm.filter}
          onChange={(e) => vm.setFilter(e.target.value)}
          placeholder="Filter…"
          className="ml-auto w-48"
        />
        <Button
          size="sm"
          onClick={vm.createFlow}
          disabled={vm.createPending}
        >
          <Plus size={14} />
          {vm.createPending ? "Creating…" : "New flow"}
        </Button>
      </header>

      {/* Error banner */}
      {vm.errorMessage && <ErrorBanner message={vm.errorMessage} />}

      {/* Content */}
      {vm.rows.length === 0 ? (
        <EmptyState hasFilter={!!vm.filter.trim()} />
      ) : (
        <FlowsTable
          rows={vm.rows}
          onOpen={vm.openFlow}
          onDelete={vm.deleteFlow}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Presentational sub-components (zero business logic)
// ---------------------------------------------------------------------------

function FlowsTable({
  rows,
  onOpen,
  onDelete,
}: {
  rows: FlowRow[];
  onOpen: (path: string) => void;
  onDelete: (path: string) => void;
}) {
  return (
    <Card className="overflow-hidden py-0">
      <CardContent className="p-0">
        <Table>
          <TableHeader>
            <TableRow className="bg-muted/50">
              <TableHead>Name</TableHead>
              <TableHead>Path</TableHead>
              <TableHead>Lifecycle</TableHead>
              <TableHead className="text-right">Nodes</TableHead>
              <TableHead className="text-right">Links</TableHead>
              <TableHead className="w-[100px]" />
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => (
              <TableRow
                key={row.id}
                onClick={() => onOpen(row.path)}
                className="cursor-pointer"
              >
                <TableCell className="font-medium">{row.name}</TableCell>
                <TableCell className="font-mono text-xs text-muted-foreground">
                  {row.path}
                </TableCell>
                <TableCell>
                  <LifecycleBadge lifecycle={row.lifecycle} />
                </TableCell>
                <TableCell className="text-right font-mono text-xs">
                  {row.nodeCount}
                </TableCell>
                <TableCell className="text-right font-mono text-xs">
                  {row.linkCount}
                </TableCell>
                <TableCell>
                  <div className="flex items-center justify-end gap-1">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="outline"
                          size="xs"
                          onClick={(e) => {
                            e.stopPropagation();
                            onOpen(row.path);
                          }}
                        >
                          <Pencil size={12} />
                          Open
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Open in editor</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="outline"
                          size="icon-xs"
                          className="text-destructive hover:bg-destructive/10"
                          onClick={(e) => {
                            e.stopPropagation();
                            onDelete(row.path);
                          }}
                        >
                          <Trash2 size={12} />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Delete flow</TooltipContent>
                    </Tooltip>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

function LifecycleBadge({ lifecycle }: { lifecycle: string }) {
  const variantMap: Record<string, "default" | "secondary" | "destructive" | "outline"> = {
    created: "outline",
    active: "default",
    disabled: "secondary",
    fault: "destructive",
  };
  return (
    <Badge variant={variantMap[lifecycle] ?? "secondary"} className="text-xs">
      {lifecycle}
    </Badge>
  );
}

function LoadingSkeleton() {
  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <div className="flex items-center gap-3">
        <Skeleton className="h-5 w-5 rounded" />
        <Skeleton className="h-5 w-16" />
        <Skeleton className="ml-auto h-8 w-48 rounded-md" />
        <Skeleton className="h-8 w-24 rounded-md" />
      </div>
      <Card className="overflow-hidden py-0">
        <CardContent className="space-y-2 p-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full rounded" />
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function EmptyState({ hasFilter }: { hasFilter: boolean }) {
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="flex flex-col items-center gap-2 text-muted-foreground">
        <GitBranch size={32} className="opacity-30" />
        <p className="text-sm">
          {hasFilter ? "No flows match your filter." : "No flows yet."}
        </p>
        {!hasFilter && (
          <p className="text-xs">
            Click <strong>New flow</strong> or seed one via{" "}
            <code>POST /api/v1/seed</code>.
          </p>
        )}
      </div>
    </div>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
      {message}
    </div>
  );
}

function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground">
      <p className="text-sm">{title}</p>
      <p className="max-w-md text-xs font-mono">{detail}</p>
    </div>
  );
}
