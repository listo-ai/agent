import { RendererList } from "../Renderer";
import type { RowNode, ColNode, GridNode } from "../types";

export function RowComponent({ node }: { node: RowNode }) {
  return (
    <div className="flex flex-row flex-wrap" style={{ gap: node.gap ?? "0.5rem" }}>
      <RendererList nodes={node.children} />
    </div>
  );
}

export function ColComponent({ node }: { node: ColNode }) {
  return (
    <div className="flex flex-col" style={{ gap: node.gap ?? "0.5rem" }}>
      <RendererList nodes={node.children} />
    </div>
  );
}

export function GridComponent({ node }: { node: GridNode }) {
  return (
    <div
      className="grid"
      style={{
        gridTemplateColumns: node.columns ?? "repeat(auto-fill, minmax(200px, 1fr))",
        gap: "0.75rem",
      }}
    >
      <RendererList nodes={node.children} />
    </div>
  );
}
