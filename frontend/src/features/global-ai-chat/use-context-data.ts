/**
 * Per-context data enrichment for the global chat panel.
 *
 * Given the current [`ChatContext`], fetches the underlying resource
 * (page node, flow node, …) via `AgentClient` so the panel can:
 *   1. **show** the user what's been loaded (visible chips + summary)
 *   2. **send** a compact summary in the assistant's system prompt.
 *
 * Cheap today, extensible tomorrow — one branch per variant. Returns
 * `null` for variants with no enrichment (list / settings / unknown)
 * so the caller renders the minimal strip without a spinner.
 */
import { useQuery } from '@tanstack/react-query'
import { useAgent } from '@/hooks/useAgent'
import type { ChatContext } from './context'

export interface ContextChip {
  /** Short label shown in the strip (e.g. "page id", "layout size"). */
  key: string
  /** Inline value text — truncated to ~40 chars in the UI. */
  value: string
}

export interface EnrichedContext {
  /** Chips rendered in the context strip. Always non-empty when this
   * hook returns a value. */
  chips: ContextChip[]
  /** Opaque summary block appended to the AI's system prompt. */
  promptAppendix?: string
  /** True while the underlying resource is being fetched. */
  loading: boolean
  /** Error message if the fetch failed — panel surfaces inline. */
  error?: string
}

export function useContextData(context: ChatContext): EnrichedContext {
  const agent = useAgent()
  const client = agent.data

  const pageId = context.kind === 'page_edit' ? context.pageId : undefined
  const flowPath = context.kind === 'flow_edit' ? context.flowPath : undefined

  const pageQuery = useQuery({
    queryKey: ['global-ai-chat', 'page', pageId] as const,
    queryFn: async () => {
      if (!client || !pageId) throw new Error('not ready')
      // Same pattern as PageBuilderPage.loadDraft — list + find-by-id.
      const resp = await client.nodes.getNodesPage({ filter: `kind=="ui.page"`, size: 500 })
      const snap = resp.data.find((n) => n.id === pageId)
      if (!snap) throw new Error(`no ui.page node with id ${pageId}`)
      return snap
    },
    enabled: client !== undefined && pageId !== undefined,
    staleTime: 10_000,
  })

  const flowQuery = useQuery({
    queryKey: ['global-ai-chat', 'flow', flowPath] as const,
    queryFn: async () => {
      if (!client || !flowPath) throw new Error('not ready')
      return client.nodes.getNode(flowPath)
    },
    enabled: client !== undefined && flowPath !== undefined,
    staleTime: 10_000,
  })

  switch (context.kind) {
    case 'page_edit': {
      const chips: ContextChip[] = [
        { key: 'route', value: `/pages/${context.pageId}/edit` },
        { key: 'page id', value: context.pageId },
      ]
      if (pageQuery.isLoading) return { chips, loading: true }
      if (pageQuery.isError) {
        return { chips, loading: false, error: (pageQuery.error as Error).message }
      }
      const snap = pageQuery.data
      if (snap) {
        chips.push({ key: 'path', value: snap.path })
        const layoutSlot = snap.slots.find((s) => s.name === 'layout')
        const layout = layoutSlot?.value as unknown
        const componentCount = countComponents(layout)
        if (componentCount > 0) {
          chips.push({ key: 'components', value: String(componentCount) })
        }
        const title = extractPageTitle(layout)
        if (title) chips.push({ key: 'title', value: title })
        return {
          chips,
          loading: false,
          promptAppendix: [
            `Page node snapshot:`,
            `- path: ${snap.path}`,
            `- id: ${snap.id}`,
            `- lifecycle: ${snap.lifecycle}`,
            layoutSlot
              ? `- layout JSON (generation ${layoutSlot.generation}):\n\`\`\`json\n${truncate(
                  JSON.stringify(layoutSlot.value ?? null, null, 2),
                  4000,
                )}\n\`\`\``
              : '- layout: (no layout slot set)',
          ].join('\n'),
        }
      }
      return { chips, loading: false }
    }

    case 'flow_edit': {
      const chips: ContextChip[] = [
        { key: 'route', value: context.nodePath
          ? `/flows/edit${context.flowPath}/${context.nodePath}`
          : `/flows/edit${context.flowPath}` },
        { key: 'flow', value: context.flowPath },
      ]
      if (context.nodePath) chips.push({ key: 'focused node', value: context.nodePath })
      if (flowQuery.isLoading) return { chips, loading: true }
      if (flowQuery.isError) {
        return { chips, loading: false, error: (flowQuery.error as Error).message }
      }
      const snap = flowQuery.data
      if (snap) {
        chips.push({ key: 'kind', value: snap.kind })
        chips.push({ key: 'lifecycle', value: snap.lifecycle })
        return {
          chips,
          loading: false,
          promptAppendix: [
            `Flow node snapshot:`,
            `- path: ${snap.path}`,
            `- kind: ${snap.kind}`,
            `- id: ${snap.id}`,
            `- lifecycle: ${snap.lifecycle}`,
            context.nodePath ? `- user is focused on nested node: ${context.nodePath}` : '',
          ]
            .filter(Boolean)
            .join('\n'),
        }
      }
      return { chips, loading: false }
    }

    case 'page_view':
      return {
        chips: [
          { key: 'route', value: `/ui/${context.pageRef}` },
          { key: 'page ref', value: context.pageRef },
        ],
        loading: false,
      }

    case 'render_view':
      return {
        chips: [
          { key: 'route', value: `/render/${context.targetId}` },
          { key: 'target node', value: context.targetId },
        ],
        loading: false,
      }

    case 'flows_list':
      return { chips: [{ key: 'route', value: '/flows' }], loading: false }
    case 'pages_list':
      return { chips: [{ key: 'route', value: '/pages' }], loading: false }
    case 'blocks':
      return { chips: [{ key: 'route', value: '/blocks' }], loading: false }
    case 'settings':
      return { chips: [{ key: 'route', value: '/settings' }], loading: false }
    case 'unknown':
      return { chips: [{ key: 'route', value: context.path }], loading: false }
  }
}

function countComponents(layout: unknown): number {
  if (!layout || typeof layout !== 'object') return 0
  let n = 0
  const walk = (v: unknown) => {
    if (!v || typeof v !== 'object') return
    const obj = v as Record<string, unknown>
    if (typeof obj['type'] === 'string') n++
    const children = obj['children']
    if (Array.isArray(children)) for (const c of children) walk(c)
    // Also walk common container fields (row/col layouts).
    for (const key of ['root', 'items', 'columns', 'rows']) {
      const val = obj[key]
      if (Array.isArray(val)) for (const c of val) walk(c)
      else if (val && typeof val === 'object') walk(val)
    }
  }
  walk(layout)
  return n
}

function extractPageTitle(layout: unknown): string | undefined {
  if (!layout || typeof layout !== 'object') return undefined
  const obj = layout as Record<string, unknown>
  const meta = obj['meta'] as Record<string, unknown> | undefined
  const metaTitle = meta?.['title']
  if (typeof metaTitle === 'string') return metaTitle
  const title = obj['title']
  if (typeof title === 'string') return title
  return undefined
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s
  return s.slice(0, max) + `\n… [truncated ${s.length - max} chars]`
}
