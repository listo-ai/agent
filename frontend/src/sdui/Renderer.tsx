/**
 * Core dispatcher — switches on `component.type` and delegates to the
 * matching implementation.  Unknown types degrade to a neutral stub so a
 * single unrecognised variant never crashes the whole tree.
 */
import type { UiComponent } from "@sys/agent-client";
import type {
  PageNode, RowNode, ColNode, GridNode, TabsNode,
  TextNode, HeadingNode, BadgeNode, DiffNode,
  TableNode, ChartNode, SparklineNode,
  TreeNode, TimelineNode, MarkdownNode,
  RichTextNode, RefPickerNode, WizardNode, DrawerNode,
  DateRangeNode, SelectNode, KpiNode,
  ButtonNode, FormNode,
  ForbiddenNode, DanglingNode, CustomNode,
} from "./types";
import { PageComponent } from "./components/Page";
import { RowComponent, ColComponent, GridComponent } from "./components/Row";
import { TabsComponent } from "./components/Tabs";
import { TextComponent } from "./components/Text";
import { HeadingComponent } from "./components/Heading";
import { BadgeComponent } from "./components/Badge";
import { DiffComponent } from "./components/Diff";
import { TableComponent } from "./components/TableComp";
import { ChartComponent } from "./components/Chart";
import { SparklineComponent } from "./components/Sparkline";
import { TreeComponent } from "./components/Tree";
import { TimelineComponent } from "./components/Timeline";
import { MarkdownComponent } from "./components/Markdown";
import { RefPickerComponent } from "./components/RefPicker";
import { WizardComponent } from "./components/Wizard";
import { DrawerComponent } from "./components/Drawer";
import { DateRangePicker } from "./components/DateRangePicker";
import { SelectComponent } from "./components/Select";
import { KpiComponent } from "./components/Kpi";
import { RichTextComponent } from "./components/RichText";
import { ButtonComponent } from "./components/ButtonComp";
import { FormComponent } from "./components/FormComp";
import { ForbiddenComponent } from "./components/Forbidden";
import { DanglingComponent } from "./components/Dangling";
import { CustomComponent } from "./components/Custom";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const as = <T,>(n: UiComponent): T => n as any as T;

export function Renderer({ node }: { node: UiComponent }) {
  switch (node.type) {
    case "page":      return <PageComponent node={as<PageNode>(node)} />;
    case "row":       return <RowComponent node={as<RowNode>(node)} />;
    case "col":       return <ColComponent node={as<ColNode>(node)} />;
    case "grid":      return <GridComponent node={as<GridNode>(node)} />;
    case "tabs":      return <TabsComponent node={as<TabsNode>(node)} />;
    case "text":      return <TextComponent node={as<TextNode>(node)} />;
    case "heading":   return <HeadingComponent node={as<HeadingNode>(node)} />;
    case "badge":     return <BadgeComponent node={as<BadgeNode>(node)} />;
    case "diff":      return <DiffComponent node={as<DiffNode>(node)} />;
    case "table":     return <TableComponent node={as<TableNode>(node)} />;
    case "chart":     return <ChartComponent node={as<ChartNode>(node)} />;
    case "sparkline": return <SparklineComponent node={as<SparklineNode>(node)} />;
    case "tree":      return <TreeComponent node={as<TreeNode>(node)} />;
    case "timeline":  return <TimelineComponent node={as<TimelineNode>(node)} />;
    case "markdown":  return <MarkdownComponent node={as<MarkdownNode>(node)} />;
    case "ref_picker":return <RefPickerComponent node={as<RefPickerNode>(node)} />;
    case "wizard":    return <WizardComponent node={as<WizardNode>(node)} />;
    case "drawer":    return <DrawerComponent node={as<DrawerNode>(node)} />;
    case "date_range":return <DateRangePicker node={as<DateRangeNode>(node)} />;
    case "select":    return <SelectComponent node={as<SelectNode>(node)} />;
    case "kpi":       return <KpiComponent node={as<KpiNode>(node)} />;
    case "rich_text": return <RichTextComponent node={as<RichTextNode>(node)} />;
    case "button":    return <ButtonComponent node={as<ButtonNode>(node)} />;
    case "form":      return <FormComponent node={as<FormNode>(node)} />;
    case "forbidden": return <ForbiddenComponent node={as<ForbiddenNode>(node)} />;
    case "dangling":  return <DanglingComponent node={as<DanglingNode>(node)} />;
    case "custom":    return <CustomComponent node={as<CustomNode>(node)} />;
    default:
      return (
        <div className="rounded border border-dashed border-muted-foreground/40 px-3 py-2 text-xs text-muted-foreground">
          Unknown component: {(node as { type: string }).type}
        </div>
      );
  }
}

/** Convenience: render a list of children */
export function RendererList({ nodes }: { nodes: UiComponent[] }) {
  return (
    <>
      {nodes.map((n, i) => (
        <Renderer key={(n as { id?: string }).id ?? i} node={n} />
      ))}
    </>
  );
}
