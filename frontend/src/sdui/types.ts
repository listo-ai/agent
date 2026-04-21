/**
 * Concrete node shapes for each UiComponent variant.
 * `UiComponent` from the client package is an index-signature interface
 * (`{ type: string; [key: string]: unknown }`), so we define each
 * variant shape explicitly here rather than using `Extract<>`.
 */
import type { UiComponent, UiAction, UiTableSource, UiTableColumn } from "@listo/agent-client";

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
export type ChartSource = { node_id: string; slot: string; field?: string };
export type ChartSeries = { label: string; points: [number, number][] };
export type ChartRange = { from: number; to: number };
export type ChartHistoryPreset = { label: string; duration_ms?: number | null };
export type ChartHistory = {
  range_ms?: number | null;
  user_selectable?: boolean;
  presets?: ChartHistoryPreset[];
};
export type ChartNode = {
  type: "chart"; id?: string; source: ChartSource;
  series: ChartSeries[]; range?: ChartRange;
  page_state_key?: string; kind?: string;
  history?: ChartHistory;
};
export type SparklineNode = {
  type: "sparkline"; id?: string; values: number[];
  subscribe?: string; intent?: string;
};
export type TreeItemShape = {
  id: string; label: string; children: TreeItemShape[]; icon?: string;
};
export type TreeNode = {
  type: "tree"; id?: string; nodes: TreeItemShape[]; node_action?: UiAction;
};
export type TimelineEvent = { ts: string; text: string; intent?: string };
export type TimelineNode = {
  type: "timeline"; id?: string; events: TimelineEvent[];
  subscribe?: string; mode?: string;
};
export type MarkdownNode = {
  type: "markdown"; id?: string; content?: string;
  subscribe?: string; mode?: string;
};
export type RefPickerNode = {
  type: "ref_picker"; id?: string; query?: string;
  value?: string; placeholder?: string;
};
export type WizardStepShape = { label: string; children: UiComponent[] };
export type WizardNode = {
  type: "wizard"; id?: string; steps: WizardStepShape[]; submit?: UiAction;
};
export type DateRangePresetShape = { label: string; duration_ms?: number | null };
export type DateRangeNode = {
  type: "date_range"; id?: string;
  page_state_key: string; presets: DateRangePresetShape[];
};
export type SelectOptionShape = { label: string; value: unknown };
export type SelectNode = {
  type: "select"; id?: string;
  page_state_key: string; options: SelectOptionShape[];
  placeholder?: string; default?: unknown;
};
export type KpiNode = {
  type: "kpi"; id?: string; label: string;
  source: ChartSource; format?: string; intent?: string;
};
export type DrawerNode = {
  type: "drawer"; id?: string; title?: string; open: boolean;
  page_state_key?: string; children: UiComponent[];
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
