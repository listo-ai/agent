import { useMemo, useState } from "react";
import type { Kind } from "@acme/agent-client";
import { facetGroup, titleForKind } from "../flow-model";

interface FlowSidebarProps {
  kinds: Kind[];
  onCreateNode: (kind: Kind) => void;
}

/**
 * Editor-side sidebar: kind palette only. The flow list lives on the
 * /flows landing page (FlowsListPage); there's no reason to duplicate
 * it here and compete with the palette for vertical space.
 */
export function FlowSidebar({ kinds, onCreateNode }: FlowSidebarProps) {
  const [search, setSearch] = useState("");

  const filteredKinds = useMemo(() => {
    const term = search.trim().toLowerCase();
    return kinds
      .filter((kind) => kind.id !== "acme.core.flow")
      .filter((kind) => {
        if (!term) return true;
        return (
          kind.id.toLowerCase().includes(term) ||
          (kind.display_name ?? "").toLowerCase().includes(term)
        );
      });
  }, [kinds, search]);

  const groups = useMemo(() => {
    const byGroup = new Map<string, Kind[]>();
    for (const kind of filteredKinds) {
      const group = facetGroup(kind);
      const list = byGroup.get(group) ?? [];
      list.push(kind);
      byGroup.set(group, list);
    }
    return [...byGroup.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [filteredKinds]);

  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-r border-border bg-card/70">
      <section className="flex min-h-0 flex-1 flex-col px-4 py-4">
        <div className="mb-3">
          <h2 className="text-sm font-semibold">Palette</h2>
          <p className="text-xs text-muted-foreground">
            Drag or click a kind to add it to the open flow.
          </p>
        </div>
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder="Search kinds"
          className="mb-3 rounded-md border border-input bg-background px-3 py-2 text-sm outline-none ring-0 placeholder:text-muted-foreground"
        />
        <div className="min-h-0 flex-1 space-y-4 overflow-auto pr-1">
          {groups.map(([group, entries]) => (
            <section key={group}>
              <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
                {group}
              </h3>
              <div className="space-y-2">
                {entries.map((kind) => (
                  <button
                    key={kind.id}
                    type="button"
                    draggable
                    onDragStart={(event) => {
                      event.dataTransfer.setData(
                        "application/x-flow-kind",
                        JSON.stringify({ kindId: kind.id }),
                      );
                      event.dataTransfer.effectAllowed = "copy";
                    }}
                    onClick={() => onCreateNode(kind)}
                    className="w-full rounded-xl border border-border bg-background px-3 py-3 text-left transition-transform hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-sm"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <div>
                        <div className="text-sm font-medium">{titleForKind(kind, kind.id)}</div>
                        <div className="mt-1 text-[11px] text-muted-foreground">{kind.id}</div>
                      </div>
                      <span className="rounded-full bg-secondary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-secondary-foreground">
                        {kind.placement_class}
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      </section>
    </aside>
  );
}
