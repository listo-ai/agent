/**
 * `drawer` component — off-canvas side panel. Open state is owned by
 * `$page[page_state_key]` so other components (buttons, menu items)
 * can toggle it through `setPageState`.
 */
import type { UiComponent } from "@listo/agent-client";
import { Renderer } from "../Renderer";
import { useSdui } from "../context";

type DrawerNodeShape = {
  type: "drawer";
  id?: string;
  title?: string;
  open: boolean;
  page_state_key?: string;
  children: UiComponent[];
};

export function DrawerComponent({ node }: { node: DrawerNodeShape }) {
  const { setPageState } = useSdui();
  const stateKey = node.page_state_key ?? `drawer_${node.id ?? "main"}`;
  if (!node.open) return null;

  return (
    <div
      className="fixed inset-0 z-50 bg-black/30"
      onClick={() => setPageState({ [stateKey]: false })}
    >
      <aside
        className="absolute right-0 top-0 flex h-full w-96 flex-col gap-3 border-l bg-background p-4 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between">
          <h3 className="text-base font-semibold">{node.title ?? ""}</h3>
          <button
            type="button"
            className="text-sm text-muted-foreground hover:text-foreground"
            onClick={() => setPageState({ [stateKey]: false })}
          >
            ✕
          </button>
        </div>
        <div className="flex flex-col gap-2">
          {node.children.map((c, i) => (
            <Renderer key={(c as { id?: string }).id ?? i} node={c} />
          ))}
        </div>
      </aside>
    </div>
  );
}
