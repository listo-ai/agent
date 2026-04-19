import type { CustomNode } from "../types";
import { useSdui } from "../context";

export function CustomComponent({ node }: { node: CustomNode }) {
  const { customRegistry } = useSdui();
  const Component = customRegistry.get(node.renderer_id);

  if (!Component) {
    return (
      <div className="flex items-center gap-2 rounded border border-dashed border-amber-400/50 bg-amber-50/20 px-3 py-2 text-sm text-muted-foreground dark:bg-amber-900/10">
        <span>🔌</span>
        <span>Custom renderer <code className="text-xs">{node.renderer_id}</code> not registered</span>
      </div>
    );
  }

  return (
    <Component
      props={node.props}
      subscribe={node.subscribe}
    />
  );
}
