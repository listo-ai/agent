/**
 * Chat driver bridging the UI lib to `AgentClient.ai`.
 *
 * Defaults to the SSE streaming endpoint (`ai.stream`) — text deltas
 * land on the assistant bubble live; tool-call frames populate
 * `toolCalls`; the terminal `result` frame flips the status to `done`.
 *
 * Opt out with `{ streaming: false }` to use the non-streaming
 * `ai.run` endpoint instead (single request/response).
 */
import { useCallback, useRef, useState } from 'react'
import type { AgentClient, AiRunRequest, AiStreamEvent } from '@sys/agent-client'
import type { ChatMessage } from './types'

export interface UseAiChatOptions {
  client: AgentClient | undefined
  provider?: string
  model?: string
  systemPrompt?: string
  maxTokens?: number
  /** Extended thinking / reasoning effort. `low|medium|high` or raw token count. */
  thinkingBudget?: string
  /** Default: true. When false, falls back to `ai.run()` (no streaming). */
  streaming?: boolean
  /** Called when the backend errors (handler throws `ClientError`). */
  onError?: (message: string) => void
}

function newId() {
  return 'msg-' + Math.random().toString(36).slice(2, 10)
}

export function useAiChat(opts: UseAiChatOptions) {
  const {
    client,
    provider,
    model,
    systemPrompt,
    maxTokens,
    thinkingBudget,
    streaming = true,
    onError,
  } = opts
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [isStreaming, setIsStreaming] = useState(false)
  const abortRef = useRef<AbortController | null>(null)

  const send = useCallback(
    async (prompt: string) => {
      if (!prompt.trim() || !client) return

      const userMsg: ChatMessage = {
        id: newId(),
        role: 'user',
        content: prompt,
        timestamp: Date.now(),
      }
      const assistantId = newId()
      const assistantMsg: ChatMessage = {
        id: assistantId,
        role: 'assistant',
        content: '',
        status: streaming ? 'connecting' : 'thinking',
        timestamp: Date.now(),
      }
      setMessages((prev) => [...prev, userMsg, assistantMsg])
      setIsStreaming(true)

      const req: AiRunRequest = {
        prompt,
        ...(systemPrompt !== undefined && { system_prompt: systemPrompt }),
        ...(provider !== undefined && provider !== '' && { provider }),
        ...(model !== undefined && model !== '' && { model }),
        ...(maxTokens !== undefined && { max_tokens: maxTokens }),
        ...(thinkingBudget !== undefined && thinkingBudget !== '' && thinkingBudget !== 'off' && {
          thinking_budget: thinkingBudget,
        }),
      }

      try {
        if (!streaming) {
          const resp = await client.ai.run(req)
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId
                ? { ...m, content: resp.text, status: 'done' as const }
                : m,
            ),
          )
          return
        }

        const abort = new AbortController()
        abortRef.current = abort
        const stream = client.ai.stream(req, { signal: abort.signal })

        let text = ''
        let toolCalls: string[] = []
        for await (const ev of stream) {
          applyEvent(ev)
        }

        function applyEvent(ev: AiStreamEvent) {
          switch (ev.type) {
            case 'connected':
              setMessages((prev) =>
                prev.map((m) => (m.id === assistantId ? { ...m, status: 'streaming' as const } : m)),
              )
              break
            case 'text':
              text += ev.content
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId ? { ...m, content: text, status: 'streaming' as const } : m,
                ),
              )
              break
            case 'tool_call':
              toolCalls = [...toolCalls, ev.name]
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? { ...m, toolCalls, status: 'tool_call' as const }
                    : m,
                ),
              )
              break
            case 'tool_use':
              // Already recorded on `tool_call`; the tool_use carries
              // the structured input which we ignore in chat UI.
              break
            case 'error':
              setMessages((prev) => [
                ...prev,
                { id: newId(), role: 'system', content: ev.message, timestamp: Date.now() },
              ])
              break
            case 'done':
              // Leave final text + status flip to the `result` frame.
              break
            case 'result':
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === assistantId
                    ? { ...m, content: ev.text || text, status: 'done' as const }
                    : m,
                ),
              )
              break
          }
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        onError?.(message)
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? { ...m, status: 'error' as const, content: m.content || '_(no response)_' }
              : m,
          ),
        )
        setMessages((prev) => [
          ...prev,
          { id: newId(), role: 'system', content: message, timestamp: Date.now() },
        ])
      } finally {
        setIsStreaming(false)
        abortRef.current = null
      }
    },
    [client, provider, model, systemPrompt, maxTokens, thinkingBudget, streaming, onError],
  )

  const clear = useCallback(() => setMessages([]), [])
  const cancel = useCallback(() => {
    abortRef.current?.abort()
  }, [])

  return { messages, isStreaming, send, clear, cancel }
}
