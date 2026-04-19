/**
 * `ref_picker` component — typeahead for selecting a graph node.
 *
 * Queries the agent via `GET /api/v1/ui/table` with the configured
 * RSQL filter, shows matches, lets the user pick one. The selected
 * node id is written to `$page[id]` so parent forms can read it
 * (form-integration is S7 polish; this is the input surface).
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { agentPromise } from "@/lib/agent";
import { useSdui } from "../context";

type RefPickerNodeShape = {
  type: "ref_picker";
  id?: string;
  query?: string;
  value?: string;
  placeholder?: string;
};

export function RefPickerComponent({ node }: { node: RefPickerNodeShape }) {
  const { setPageState } = useSdui();
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<string | undefined>(node.value);

  const { data } = useQuery({
    queryKey: ["sdui-ref-picker", node.id, node.query, q],
    queryFn: async () => {
      const client = await agentPromise;
      // Combine the IR's base query with the user's text search on path.
      const filter = q ? `path=prefix=${q}` : undefined;
      return client.ui.table({
        query: node.query ?? "",
        filter,
        page: 1,
        size: 20,
      });
    },
  });

  return (
    <div className="flex flex-col gap-1 text-sm">
      <input
        type="text"
        className="rounded border bg-background px-2 py-1"
        placeholder={node.placeholder ?? "Search nodes…"}
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      {selected ? (
        <div className="text-xs text-muted-foreground">
          Selected: <code>{selected}</code>
        </div>
      ) : null}
      <ul className="max-h-48 overflow-auto rounded border">
        {(data?.data ?? []).map((row) => (
          <li
            key={row.id}
            className={`cursor-pointer px-2 py-1 hover:bg-muted/50 ${selected === row.id ? "bg-muted" : ""}`}
            onClick={() => {
              setSelected(row.id);
              if (node.id) setPageState({ [node.id]: row.id });
            }}
          >
            <div>{row.path}</div>
            <div className="text-xs text-muted-foreground">{row.kind}</div>
          </li>
        ))}
      </ul>
    </div>
  );
}
