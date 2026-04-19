/**
 * `tree` component — hierarchical list with expand/collapse.
 */
import { useState } from "react";
import { useSdui } from "../context";
import type { UiAction } from "@sys/agent-client";

type TreeItem = {
  id: string;
  label: string;
  children: TreeItem[];
  icon?: string | undefined;
};
type TreeNodeShape = {
  type: "tree";
  id?: string;
  nodes: TreeItem[];
  node_action?: UiAction;
};

export function TreeComponent({ node }: { node: TreeNodeShape }) {
  const { dispatchAction } = useSdui();
  return (
    <ul className="flex flex-col gap-0.5 text-sm">
      {node.nodes.map((n) => (
        <TreeRow
          key={n.id}
          item={n}
          depth={0}
          onClick={(item) => {
            if (!node.node_action) return;
            void dispatchAction(node.node_action.handler, {
              ...(typeof node.node_action.args === "object" &&
              node.node_action.args !== null
                ? node.node_action.args
                : {}),
              $node: { id: item.id, label: item.label },
            });
          }}
        />
      ))}
    </ul>
  );
}

function TreeRow({
  item,
  depth,
  onClick,
}: {
  item: TreeItem;
  depth: number;
  onClick: (item: TreeItem) => void;
}) {
  const [open, setOpen] = useState(true);
  const hasChildren = (item.children ?? []).length > 0;
  return (
    <li>
      <div
        role="treeitem"
        aria-expanded={hasChildren ? open : undefined}
        className="flex items-center gap-1 rounded px-1 py-0.5 hover:bg-muted/50 cursor-pointer"
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
        onClick={() => onClick(item)}
      >
        {hasChildren ? (
          <button
            type="button"
            className="w-4 text-xs text-muted-foreground"
            onClick={(e) => {
              e.stopPropagation();
              setOpen((o) => !o);
            }}
          >
            {open ? "▾" : "▸"}
          </button>
        ) : (
          <span className="w-4" />
        )}
        <span>{item.label}</span>
      </div>
      {hasChildren && open ? (
        <ul>
          {item.children.map((c) => (
            <TreeRow key={c.id} item={c} depth={depth + 1} onClick={onClick} />
          ))}
        </ul>
      ) : null}
    </li>
  );
}
