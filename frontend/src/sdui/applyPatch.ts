/**
 * Tree-patch helpers — used by both optimistic action hints and
 * authoritative `Patch` / `FullRender` responses.
 *
 * A patch targets a single component by its authored `id`: we walk the
 * tree, find it, and shallow-merge the given fields (or replace the
 * subtree for FullRender-style replacement). Everything else stays
 * pointer-identical so React only re-renders the affected branch.
 */
import type { UiComponent, UiComponentTree } from "@listo/agent-client";

/** Shallow-merge `fields` into the component with the matching `id`. */
export function mergeAt(
  tree: UiComponentTree,
  targetId: string,
  fields: Record<string, unknown>,
): UiComponentTree {
  return { ...tree, root: mergeNode(tree.root, targetId, fields) };
}

/** Replace a subtree rooted at the component with the matching `id`. */
export function replaceAt(
  tree: UiComponentTree,
  targetId: string,
  replacement: UiComponent,
): UiComponentTree {
  return { ...tree, root: replaceNode(tree.root, targetId, replacement) };
}

function mergeNode(
  node: UiComponent,
  targetId: string,
  fields: Record<string, unknown>,
): UiComponent {
  if ((node as { id?: string }).id === targetId) {
    return { ...node, ...fields } as UiComponent;
  }
  const children = (node as { children?: UiComponent[] }).children;
  if (Array.isArray(children)) {
    const next = children.map((c) => mergeNode(c, targetId, fields));
    if (next.some((c, i) => c !== children[i])) {
      return { ...node, children: next } as UiComponent;
    }
  }
  const tabs = (node as { tabs?: { children: UiComponent[] }[] }).tabs;
  if (Array.isArray(tabs)) {
    const next = tabs.map((t) => ({
      ...t,
      children: t.children.map((c) => mergeNode(c, targetId, fields)),
    }));
    if (next.some((t, i) => t !== tabs[i])) {
      return { ...node, tabs: next } as UiComponent;
    }
  }
  return node;
}

function replaceNode(
  node: UiComponent,
  targetId: string,
  replacement: UiComponent,
): UiComponent {
  if ((node as { id?: string }).id === targetId) return replacement;
  const children = (node as { children?: UiComponent[] }).children;
  if (Array.isArray(children)) {
    const next = children.map((c) => replaceNode(c, targetId, replacement));
    if (next.some((c, i) => c !== children[i])) {
      return { ...node, children: next } as UiComponent;
    }
  }
  return node;
}
