/**
 * Build the system-prompt addition that tells the AI where the user is
 * and what they're likely working on. Pure fn — no React, no fetch.
 *
 * We intentionally keep this shallow: we tell the model **what the user
 * is looking at** (route, identifiers), but we do not fetch the
 * underlying page/flow JSON here. That's a follow-up — the panel can
 * enrich on a per-context basis before sending.
 */
import type { ChatContext } from './context'

const BASE = [
  'You are the Studio assistant — an AI integrated into a graph-based',
  'automation platform. The user runs flows, authors dashboard pages,',
  'and manages nodes. Keep answers short and concrete. When the user',
  'asks "this page" / "this flow" / "here", default to the context',
  "section below — that's where they currently are.",
].join(' ')

export function buildSystemPrompt(ctx: ChatContext, extra?: string): string {
  const header = BASE
  const location = contextSection(ctx)
  const tail = extra?.trim() ? `\n\nAdditional context from the user:\n${extra.trim()}` : ''
  return `${header}\n\n${location}${tail}`
}

function contextSection(ctx: ChatContext): string {
  switch (ctx.kind) {
    case 'page_edit':
      return [
        'Current location: **page editor**.',
        `- page id: \`${ctx.pageId}\``,
        '- screen: visual + JSON composer for a `ui.page.layout` node',
        '- typical asks: "add a KPI row", "explain these bindings",',
        '  "fix this validation error".',
      ].join('\n')
    case 'page_view':
      return [
        'Current location: **rendered dashboard page**.',
        `- page reference: \`${ctx.pageRef}\``,
        '- screen: live render of the page for end users',
        '- typical asks: "why is this table empty?", "what slot drives',
        '  this chart?"',
      ].join('\n')
    case 'render_view':
      return [
        'Current location: **kind-view renderer**.',
        `- target node id: \`${ctx.targetId}\``,
        '- screen: default SDUI view for the target\'s kind',
      ].join('\n')
    case 'flows_list':
      return [
        'Current location: **flows list**.',
        '- screen: index of all flow documents the agent knows about',
        '- typical asks: "what flows are running?", "create a new flow',
        '  for X".',
      ].join('\n')
    case 'flow_edit':
      return [
        'Current location: **flow editor canvas**.',
        `- flow path: \`${ctx.flowPath}\``,
        ctx.nodePath ? `- focused node: \`${ctx.nodePath}\`` : '- no node focused yet',
        '- typical asks: "wire this sensor to that alarm", "explain',
        '  this node\'s settings", "add a trigger upstream of X".',
      ].join('\n')
    case 'pages_list':
      return 'Current location: **pages list**. Index of all dashboard pages.'
    case 'blocks':
      return 'Current location: **blocks page**. Process-block host management.'
    case 'settings':
      return 'Current location: **settings**.'
    case 'unknown':
      return `Current location: \`${ctx.path}\` (unrecognised route).`
  }
}
