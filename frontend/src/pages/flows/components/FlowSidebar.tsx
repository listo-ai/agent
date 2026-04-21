import { useMemo, useState } from "react";
import type { Kind } from "@listo/agent-client";
import { facetGroup, titleForKind } from "../flow-model";
import { Badge, Input } from "@/components/ui";

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
      .filter((kind) => kind.id !== "sys.core.flow")
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
        <Input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder="Search kinds"
          className="mb-3"
        />
        <div className="min-h-0 flex-1 overflow-y-auto pr-1">
          <div className="space-y-4">
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
                        <Badge variant="secondary" className="text-[10px] uppercase tracking-wide">
                          {kind.placement_class}
                        </Badge>
                      </div>
                    </button>
                  ))}
                </div>
              </section>
            ))}
          </div>
        </div>
      </section>
    </aside>
  );
}
