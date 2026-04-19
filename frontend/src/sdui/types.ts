/**
 * Concrete node shapes for each UiComponent variant.
 * `UiComponent` from the client package is an index-signature interface
 * (`{ type: string; [key: string]: unknown }`), so we define each
 * variant shape explicitly here rather than using `Extract<>`.
 */
import type { UiComponent, UiAction, UiTableSource, UiTableColumn } from "@sys/agent-client";

export type PageNode = {
  type: "page"; id: string; title?: string | null; children: UiComponent[];
};
export type RowNode = {
  type: "row"; id?: string; children: UiComponent[]; gap?: string;
};
export type ColNode = {
  type: "col"; id?: string; children: UiComponent[]; gap?: string;
};
export type GridNode = {
  type: "grid"; id?: string; children: UiComponent[]; columns?: string;
};
export type Tab = { id?: string; label: string; children: UiComponent[] };
export type TabsNode = {
  type: "tabs"; id?: string; tabs: Tab[];
};
export type TextNode = {
  type: "text"; id?: string; content: string; intent?: string;
};
export type HeadingNode = {
  type: "heading"; id?: string; content: string; level?: number;
};
export type BadgeNode = {
  type: "badge"; id?: string; label: string; intent?: string;
};
export type DiffAnnotation = { line: number; text: string; author?: string; created_at?: string };
export type DiffNode = {
  type: "diff"; id?: string; old_text: string; new_text: string;
  language?: string; annotations: DiffAnnotation[]; line_action?: UiAction;
};
export type TableNode = {
  type: "table"; id?: string; source: UiTableSource;
  columns: UiTableColumn[]; row_action?: UiAction; page_size?: number;
};
export type RichTextNode = {
  type: "rich_text"; id?: string; value?: string; placeholder?: string;
};
export type ButtonNode = {
  type: "button"; id?: string; label: string; intent?: string;
  disabled?: boolean; action?: UiAction;
};
export type FormNode = {
  type: "form"; id?: string; schema_ref: string;
  bindings?: unknown; submit?: UiAction;
};
export type ForbiddenNode = {
  type: "forbidden"; id: string; reason: string;
};
export type DanglingNode = {
  type: "dangling"; id: string;
};
export type CustomNode = {
  type: "custom"; id?: string; renderer_id: string; props?: unknown; subscribe: string[];
};

/** Cast a `UiComponent` to a specific node variant type. */
export function asNode<T>(node: UiComponent): T {
  return node as unknown as T;
}
