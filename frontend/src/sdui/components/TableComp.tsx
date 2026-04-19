/**
 * `table` component — fetches rows from `GET /api/v1/ui/table` and renders
 * them as a paginated table.  The source's `query` field is used as the RSQL
 * filter; `columns` drives the header + cell accessor expressions.
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Button } from "@/components/ui/button";
import type { UiTableRow } from "@sys/agent-client";
import type { TableNode } from "../types";
import { agentPromise } from "@/lib/agent";
import { useSdui } from "../context";

/** Safely traverse a dot-path on an object (e.g. "slots.value.v"). */
function getPath(obj: unknown, path: string): string {
  const parts = path.split(".");
  let cur: unknown = obj;
  for (const p of parts) {
    if (cur == null || typeof cur !== "object") return "";
    cur = (cur as Record<string, unknown>)[p];
  }
  return cur == null ? "" : String(cur);
}

export function TableComponent({ node }: { node: TableNode }) {
  const [page, setPage] = useState(1);
  const { dispatchAction } = useSdui();
  const pageSize = node.page_size ?? 50;

  const { data, isLoading, isError } = useQuery({
    queryKey: ["sdui-table", node.id, node.source.query, page, pageSize],
    queryFn: async () => {
      const client = await agentPromise;
      return client.ui.table({
        query: node.source.query ?? "",
        page,
        size: pageSize,
      });
    },
    staleTime: 30_000,
  });

  if (isLoading) return <div className="text-sm text-muted-foreground">Loading…</div>;
  if (isError || !data) return <div className="text-sm text-destructive">Failed to load table.</div>;

  const { data: rows, meta } = data;

  function handleRowAction(row: UiTableRow) {
    if (!node.row_action) return;
    void dispatchAction(node.row_action.handler, {
      ...(typeof node.row_action.args === "object" && node.row_action.args !== null ? node.row_action.args : {}),
      $row: row,
    });
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="overflow-auto rounded border">
        <Table>
          <TableHeader>
            <TableRow>
              {node.columns.map((col) => (
                <TableHead key={col.field}>{col.title}</TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={node.columns.length}
                  className="text-center text-muted-foreground"
                >
                  No rows
                </TableCell>
              </TableRow>
            ) : (
              rows.map((row) => (
                <TableRow
                  key={row.id}
                  className={node.row_action ? "cursor-pointer hover:bg-muted/50" : undefined}
                  onClick={() => handleRowAction(row)}
                >
                  {node.columns.map((col) => (
                    <TableCell key={col.field}>
                      {getPath(row, col.field)}
                    </TableCell>
                  ))}
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      {meta.pages > 1 && (
        <div className="flex items-center justify-end gap-2 text-sm">
          <span className="text-muted-foreground">
            Page {meta.page} of {meta.pages} ({meta.total} rows)
          </span>
          <Button
            size="sm"
            variant="outline"
            disabled={page <= 1}
            onClick={() => setPage((p) => p - 1)}
          >
            Prev
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={page >= meta.pages}
            onClick={() => setPage((p) => p + 1)}
          >
            Next
          </Button>
        </div>
      )}
    </div>
  );
}
