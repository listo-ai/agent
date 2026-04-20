/**
 * Floating AI chat panel, mounted once in [`Shell`].
 *
 * The **trigger** lives in the [`SiteHeader`] (not this file) — it
 * calls `useGlobalAiChat.toggle()` to open the Sheet rendered here.
 *
 * Wires the generic chat lib (`@/lib/chat`) to the shared
 * `AgentClient.ai` surface, injecting a route-aware system prompt
 * and a compact snapshot of the current page / flow so questions
 * like "explain this page" get a real answer.
 */
import { useRef, useState } from 'react'
import { Sparkles, MapPin, Loader2, AlertCircle } from 'lucide-react'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '@/components/ui'
import { ChatBubble, ChatInput, ChatSuggestions, useAiChat, useAutoScroll } from '@/lib/chat'
import { useAgent } from '@/hooks/useAgent'
import { useGlobalAiChat } from './store'
import { type ChatContext } from './context'
import { buildSystemPrompt } from './prompt'
import { useContextData, type ContextChip } from './use-context-data'
import { RunnerSettings } from './RunnerSettings'

export function GlobalAiChat() {
  const open = useGlobalAiChat((s) => s.open)
  const setOpen = useGlobalAiChat((s) => s.setOpen)
  const context = useGlobalAiChat((s) => s.context)
  const extraHints = useGlobalAiChat((s) => s.extraHints)
  const setExtraHints = useGlobalAiChat((s) => s.setExtraHints)

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetContent
        side="right"
        className="w-[440px] sm:max-w-[440px] p-0 flex flex-col gap-0"
      >
        <SheetHeader className="px-3 py-2 border-b border-border space-y-0">
          <SheetTitle className="text-xs font-mono tracking-wider uppercase text-muted-foreground flex items-center gap-1.5">
            <Sparkles size={12} /> AI assistant
          </SheetTitle>
        </SheetHeader>

        <ContextStrip context={context} hints={extraHints} onHintsChange={setExtraHints} />
        <RunnerSettings />

        <ChatPane key={contextKey(context)} context={context} extraHints={extraHints} />
      </SheetContent>
    </Sheet>
  )
}

function contextKey(ctx: ChatContext): string {
  // Remount the pane when the context changes category so stale
  // conversations don't bleed into a new focus.
  switch (ctx.kind) {
    case 'page_edit':
      return `page_edit:${ctx.pageId}`
    case 'page_view':
      return `page_view:${ctx.pageRef}`
    case 'render_view':
      return `render_view:${ctx.targetId}`
    case 'flow_edit':
      return `flow_edit:${ctx.flowPath}:${ctx.nodePath ?? ''}`
    default:
      return ctx.kind
  }
}

function ContextStrip({
  context,
  hints,
  onHintsChange,
}: {
  context: ChatContext
  hints: string
  onHintsChange: (value: string) => void
}) {
  const [showHints, setShowHints] = useState(false)
  const { chips, loading, error } = useContextData(context)

  return (
    <div className="border-b border-border px-3 py-2 bg-muted/30 space-y-1.5">
      <div className="flex items-center gap-1.5 text-[10px] font-mono uppercase tracking-wider text-muted-foreground/70">
        <MapPin size={10} />
        <span>Context the AI will see</span>
        {loading && <Loader2 size={10} className="animate-spin ml-auto" />}
      </div>

      {error && (
        <div className="flex items-start gap-1.5 text-[11px] text-destructive">
          <AlertCircle size={11} className="mt-0.5 shrink-0" />
          <span className="font-mono break-all">{error}</span>
        </div>
      )}

      <ul className="space-y-0.5">
        {chips.map((chip) => (
          <ChipRow key={chip.key} chip={chip} />
        ))}
      </ul>

      <button
        onClick={() => setShowHints((p) => !p)}
        className="text-[10px] font-mono underline underline-offset-2 text-muted-foreground hover:text-foreground"
      >
        {showHints ? 'hide hints' : hints ? `edit hints (${hints.length} chars)` : 'add hints'}
      </button>
      {showHints && (
        <textarea
          value={hints}
          onChange={(e) => onHintsChange(e.target.value)}
          rows={2}
          placeholder="Extra context to prepend to every message (node ids, intent, constraints)…"
          className="w-full bg-background border border-border text-xs p-1.5 resize-y focus:outline-none focus:border-foreground/30"
        />
      )}
    </div>
  )
}

function ChipRow({ chip }: { chip: ContextChip }) {
  return (
    <li className="flex items-center gap-2 text-[11px] font-mono">
      <span className="text-muted-foreground shrink-0 min-w-[70px]">{chip.key}</span>
      <span className="text-foreground truncate" title={chip.value}>
        {chip.value}
      </span>
    </li>
  )
}

function ChatPane({ context, extraHints }: { context: ChatContext; extraHints: string }) {
  const agent = useAgent()
  const scrollRef = useRef<HTMLDivElement>(null)
  const { promptAppendix } = useContextData(context)
  const provider = useGlobalAiChat((s) => s.provider)
  const model = useGlobalAiChat((s) => s.model)
  const thinkingBudget = useGlobalAiChat((s) => s.thinkingBudget)

  const systemPrompt = buildSystemPrompt(
    context,
    [extraHints, promptAppendix].filter(Boolean).join('\n\n'),
  )

  const { messages, isStreaming, send, clear } = useAiChat({
    client: agent.data,
    systemPrompt,
    ...(provider !== '' && { provider }),
    ...(model !== '' && { model }),
    ...(thinkingBudget !== 'off' && { thinkingBudget }),
  })
  const [input, setInput] = useState('')

  useAutoScroll(scrollRef, [messages, isStreaming])

  const handleSend = () => {
    const v = input.trim()
    if (!v) return
    setInput('')
    void send(v)
  }

  const suggestions = defaultSuggestions(context)

  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
        {messages.length === 0 ? (
          <ChatSuggestions
            compact
            suggestions={suggestions}
            onSelect={(p) => void send(p)}
            title="What can I help with?"
            icon={<Sparkles size={20} className="text-muted-foreground" />}
          />
        ) : (
          messages.map((m, i) => (
            <ChatBubble
              key={m.id}
              message={m}
              isLast={i === messages.length - 1}
              isStreaming={isStreaming}
              compact
            />
          ))
        )}
      </div>
      <div className="px-3 py-2 border-t border-border">
        <ChatInput
          value={input}
          onChange={setInput}
          onSend={handleSend}
          onClear={messages.length > 0 ? clear : undefined}
          isStreaming={isStreaming}
          compact
          placeholder="Ask about this view…"
        />
      </div>
    </div>
  )
}

function defaultSuggestions(ctx: ChatContext): { label: string; prompt: string }[] {
  switch (ctx.kind) {
    case 'page_edit':
      return [
        { label: 'Explain this page', prompt: 'Explain what this page renders and which slots it binds to.' },
        { label: 'Suggest improvements', prompt: 'What would make this page more useful? Be concrete.' },
        { label: 'Add a KPI row', prompt: 'Add a KPI row at the top showing the most relevant metrics.' },
      ]
    case 'page_view':
      return [
        { label: 'Why is this page empty?', prompt: 'This page looks empty — what should I check?' },
        { label: 'Explain the data flow', prompt: 'Explain which nodes feed this page and how the bindings resolve.' },
      ]
    case 'flow_edit':
      return [
        { label: 'Explain this flow', prompt: 'Explain what this flow does in plain language.' },
        { label: 'Add a trigger', prompt: 'Help me add a trigger upstream of the current node.' },
        { label: 'Debug this flow', prompt: 'This flow isn’t behaving as expected — walk me through debugging it.' },
      ]
    case 'flows_list':
      return [
        { label: 'What flows do I have?', prompt: 'Summarise the flows that exist on this agent.' },
        { label: 'Create a flow', prompt: 'Help me create a new flow for monitoring a single sensor.' },
      ]
    default:
      return [
        { label: 'What can you do?', prompt: 'What can you help me with from this screen?' },
        { label: 'List AI providers', prompt: 'List the available AI providers on this agent.' },
      ]
  }
}
