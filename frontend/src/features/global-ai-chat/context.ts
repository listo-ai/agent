/**
 * Typed chat context — what the assistant "sees" based on the user's
 * current route. Produced by [`parseRoute`], consumed by the panel's
 * prompt builder.
 *
 * Keep this union additive: new routes get a new variant; the fallback
 * `Unknown` catches everything we haven't modelled so the panel still
 * renders (it just won't inject as much context).
 */

export type ChatContext =
  | { kind: 'page_edit'; pageId: string }
  | { kind: 'page_view'; pageRef: string }
  | { kind: 'render_view'; targetId: string }
  | { kind: 'flows_list' }
  | { kind: 'flow_edit'; flowPath: string; nodePath?: string }
  | { kind: 'pages_list' }
  | { kind: 'blocks' }
  | { kind: 'settings' }
  | { kind: 'unknown'; path: string }

/**
 * Parse a react-router location into a structured [`ChatContext`].
 *
 * Tight matching: we strip the leading slash, split, and handle each
 * route family explicitly so the resulting context mirrors what the
 * router itself recognises (see [`router.tsx`](../../router.tsx)).
 */
export function parseRoute(pathname: string): ChatContext {
  const raw = pathname.replace(/^\/+/, '').replace(/\/+$/, '')
  if (raw === '' || raw === 'flows') return { kind: 'flows_list' }
  if (raw === 'pages') return { kind: 'pages_list' }
  if (raw === 'blocks') return { kind: 'blocks' }
  if (raw === 'settings') return { kind: 'settings' }

  const parts = raw.split('/')

  // pages/:id/edit
  if (parts[0] === 'pages' && parts.length >= 3 && parts[2] === 'edit' && parts[1]) {
    return { kind: 'page_edit', pageId: parts[1] }
  }

  // ui/:pageRef
  if (parts[0] === 'ui' && parts.length >= 2 && parts[1]) {
    return { kind: 'page_view', pageRef: parts.slice(1).join('/') }
  }

  // render/:targetId
  if (parts[0] === 'render' && parts.length >= 2 && parts[1]) {
    return { kind: 'render_view', targetId: parts[1] }
  }

  // flows/edit/<flow-path>[/<node-path>]
  if (parts[0] === 'flows' && parts[1] === 'edit' && parts.length >= 3) {
    const tail = parts.slice(2)
    // Heuristic: first segment is the flow path, anything after is the
    // nested node path inside that flow's canvas. Matches the router's
    // `flows/edit/*` splat convention.
    const [flowPath, ...nodeSegs] = tail
    return {
      kind: 'flow_edit',
      flowPath: '/' + (flowPath ?? ''),
      ...(nodeSegs.length > 0 && { nodePath: nodeSegs.join('/') }),
    }
  }

  // flows/<flow-path>  (short-form deep link)
  if (parts[0] === 'flows' && parts.length >= 2) {
    return { kind: 'flow_edit', flowPath: '/' + parts.slice(1).join('/') }
  }

  return { kind: 'unknown', path: pathname }
}

/** Short human-readable label for the header strip. */
export function contextLabel(ctx: ChatContext): string {
  switch (ctx.kind) {
    case 'page_edit':
      return `Editing page ${short(ctx.pageId)}`
    case 'page_view':
      return `Viewing page ${short(ctx.pageRef)}`
    case 'render_view':
      return `Viewing kind-view of ${short(ctx.targetId)}`
    case 'flows_list':
      return 'Flows list'
    case 'flow_edit':
      return ctx.nodePath
        ? `Flow ${ctx.flowPath} · ${ctx.nodePath}`
        : `Flow ${ctx.flowPath}`
    case 'pages_list':
      return 'Pages list'
    case 'blocks':
      return 'Blocks'
    case 'settings':
      return 'Settings'
    case 'unknown':
      return ctx.path
  }
}

function short(id: string): string {
  return id.length > 12 ? id.slice(0, 8) + '…' : id
}
