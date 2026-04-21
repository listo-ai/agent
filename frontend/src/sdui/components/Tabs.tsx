import { Tabs, TabsContent, TabsList, TabsTrigger } from "@listo/ui-kit";
import { Renderer } from "../Renderer";
import type { TabsNode, Tab } from "../types";

export function TabsComponent({ node }: { node: TabsNode }) {
  const tabs = node.tabs as Tab[];
  if (!tabs.length) return null;
  const defaultTab = tabs[0];
  if (!defaultTab) return null;
  return (
    <Tabs defaultValue={defaultTab.id ?? defaultTab.label}>
      <TabsList>
        {tabs.map((t) => (
          <TabsTrigger key={t.id ?? t.label} value={t.id ?? t.label}>
            {t.label}
          </TabsTrigger>
        ))}
      </TabsList>
      {tabs.map((t) => (
        <TabsContent key={t.id ?? t.label} value={t.id ?? t.label}>
          <div className="flex flex-col gap-3">
            {t.children.map((c, i) => (
              <Renderer key={(c as { id?: string }).id ?? i} node={c} />
            ))}
          </div>
        </TabsContent>
      ))}
    </Tabs>
  );
}
