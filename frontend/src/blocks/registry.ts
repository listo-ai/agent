import type {
  BlockManifest,
  ExtensionNodeContribution,
  ExtensionViewContribution,
  ExtensionWidgetContribution,
} from "./types";

// In-memory registry of all loaded block contributions.
// Populated by the loader after each block is successfully loaded.

interface ExtensionRegistry {
  nodes:   Map<string, ExtensionNodeContribution & { extensionId: string }>;
  views:   Map<string, ExtensionViewContribution  & { extensionId: string }>;
  widgets: Map<string, ExtensionWidgetContribution & { extensionId: string }>;
}

export const extensionRegistry: ExtensionRegistry = {
  nodes:   new Map(),
  views:   new Map(),
  widgets: new Map(),
};

export function registerExtensionContributions(manifest: BlockManifest): void {
  for (const node of manifest.contributions.nodes ?? []) {
    extensionRegistry.nodes.set(node.id, { ...node, extensionId: manifest.id });
  }
  for (const view of manifest.contributions.views ?? []) {
    extensionRegistry.views.set(view.id, { ...view, extensionId: manifest.id });
  }
  for (const widget of manifest.contributions.widgets ?? []) {
    extensionRegistry.widgets.set(widget.id, { ...widget, extensionId: manifest.id });
  }
}

export function unregisterExtensionContributions(extensionId: string): void {
  for (const [k, v] of extensionRegistry.nodes)   { if (v.extensionId === extensionId) extensionRegistry.nodes.delete(k);   }
  for (const [k, v] of extensionRegistry.views)   { if (v.extensionId === extensionId) extensionRegistry.views.delete(k);   }
  for (const [k, v] of extensionRegistry.widgets) { if (v.extensionId === extensionId) extensionRegistry.widgets.delete(k); }
}
