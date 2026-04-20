/**
 * Minimal POST-based Server-Sent Events client.
 *
 * `EventSource` (used by `transport/sse.ts` for the main event stream)
 * only supports GET. For endpoints that take a JSON request body —
 * `POST /api/v1/ai/stream` etc. — we consume the response via `fetch`
 * + `ReadableStream` and parse SSE frames by hand.
 *
 * Each event's `data:` line is emitted as the raw JSON string; the
 * caller's zod schema is responsible for validation. Unparseable
 * frames are dropped (forward-compat).
 */

export interface SsePostOptions {
  url: string
  body: unknown
  token?: string | undefined
  /** AbortSignal for cancellation (e.g. component unmount). */
  signal?: AbortSignal | undefined
}

/** Open a POST SSE stream and yield each `data:` payload as a string. */
export async function* streamSsePost(opts: SsePostOptions): AsyncGenerator<string, void, void> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Accept: 'text/event-stream',
  }
  if (opts.token) headers['Authorization'] = `Bearer ${opts.token}`

  const res = await fetch(opts.url, {
    method: 'POST',
    headers,
    body: JSON.stringify(opts.body),
    ...(opts.signal !== undefined && { signal: opts.signal }),
  })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`HTTP ${res.status}: ${body || res.statusText}`)
  }
  if (!res.body) {
    throw new Error('response has no body')
  }

  const reader = res.body.getReader()
  const decoder = new TextDecoder('utf-8')
  let buf = ''

  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) {
        // Flush any trailing frame without terminator.
        const tail = popFrame(buf + '\n\n')
        if (tail) yield tail.data
        return
      }
      buf += decoder.decode(value, { stream: true })

      let next = popFrame(buf)
      while (next) {
        buf = next.rest
        if (next.data) yield next.data
        next = popFrame(buf)
      }
    }
  } finally {
    reader.releaseLock()
  }
}

interface Frame {
  data: string
  rest: string
}

/**
 * Pop the next SSE event from `buf`. Events end at a blank line.
 * Multi-line `data:` payloads are joined with `\n` per the SSE spec.
 * Non-`data:` lines (`event:`, `id:`, `retry:`) are ignored — the
 * wire encodes the event's `type` inside the JSON payload.
 */
function popFrame(buf: string): Frame | null {
  // Look for LF-LF or CRLF-CRLF separator.
  let sepIdx = -1
  let sepLen = 0
  const lfLf = buf.indexOf('\n\n')
  const crLfCrLf = buf.indexOf('\r\n\r\n')
  if (lfLf === -1 && crLfCrLf === -1) return null
  if (lfLf !== -1 && (crLfCrLf === -1 || lfLf < crLfCrLf)) {
    sepIdx = lfLf
    sepLen = 2
  } else {
    sepIdx = crLfCrLf
    sepLen = 4
  }

  const head = buf.slice(0, sepIdx)
  const rest = buf.slice(sepIdx + sepLen)

  const dataLines: string[] = []
  for (const rawLine of head.split('\n')) {
    const line = rawLine.endsWith('\r') ? rawLine.slice(0, -1) : rawLine
    if (line.startsWith('data:')) {
      const v = line.slice(5)
      dataLines.push(v.startsWith(' ') ? v.slice(1) : v)
    }
    // Ignore event:, id:, retry:, comments — the JSON carries `type`.
  }
  return { data: dataLines.join('\n'), rest }
}
