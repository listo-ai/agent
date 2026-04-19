import { RendererList } from "../Renderer";
import type { PageNode } from "../types";

export function PageComponent({ node }: { node: PageNode }) {
  return (
    <div className="flex flex-col gap-4 p-6">
      {node.title && (
        <h1 className="text-lg font-semibold">{node.title}</h1>
      )}
      <div className="flex flex-col gap-3">
        <RendererList nodes={node.children} />
      </div>
    </div>
  );
}
