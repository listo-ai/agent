/**
 * AI runner settings strip — provider + model + reasoning effort.
 *
 * Reads live provider availability from `GET /api/v1/ai/providers` so
 * the dropdown reflects what the agent actually supports. Selections
 * persist via the [`useGlobalAiChat`] zustand store, so the user's
 * choice survives a route change or reload.
 */
import { useQuery } from '@tanstack/react-query'
import { Cpu, ChevronDown } from 'lucide-react'
import { useState } from 'react'
import { useAgent } from '@/hooks/useAgent'
import { useGlobalAiChat, type ThinkingEffort } from './store'

const EFFORTS: { value: ThinkingEffort; label: string }[] = [
  { value: 'off', label: 'off' },
  { value: 'low', label: 'low' },
  { value: 'medium', label: 'medium' },
  { value: 'high', label: 'high' },
]

export function RunnerSettings() {
  const agent = useAgent()
  const provider = useGlobalAiChat((s) => s.provider)
  const model = useGlobalAiChat((s) => s.model)
  const thinkingBudget = useGlobalAiChat((s) => s.thinkingBudget)
  const setProvider = useGlobalAiChat((s) => s.setProvider)
  const setModel = useGlobalAiChat((s) => s.setModel)
  const setThinkingBudget = useGlobalAiChat((s) => s.setThinkingBudget)

  const [expanded, setExpanded] = useState(false)

  const providersQuery = useQuery({
    queryKey: ['ai', 'providers'] as const,
    queryFn: () => agent.data!.ai.providers(),
    enabled: agent.data !== undefined,
    staleTime: 60_000,
  })
  const providers = providersQuery.data ?? []

  const effectiveProvider = provider || 'agent default'
  const effectiveModel = model || 'runner default'
  const effectiveEffort = thinkingBudget

  return (
    <div className="border-b border-border px-3 py-2 bg-background">
      {/* Collapsed one-liner summary (always visible). */}
      <button
        onClick={() => setExpanded((p) => !p)}
        className="w-full flex items-center gap-2 text-[10px] font-mono uppercase tracking-wider text-muted-foreground/70 hover:text-foreground"
      >
        <Cpu size={10} />
        <span>Runner</span>
        <span className="text-foreground normal-case tracking-normal truncate">
          {effectiveProvider}
          {model ? ` · ${effectiveModel}` : ''}
          {effectiveEffort !== 'off' ? ` · think ${effectiveEffort}` : ''}
        </span>
        <ChevronDown
          size={12}
          className={`ml-auto transition-transform ${expanded ? 'rotate-180' : ''}`}
        />
      </button>

      {expanded && (
        <div className="mt-2 space-y-2">
          {/* Provider row — `available:false` is informational, not a block.
              Agents launched outside a shell often don't see CLI binaries on
              PATH; the user may know better than the probe. */}
          <Field label="provider">
            <select
              value={provider}
              onChange={(e) => setProvider(e.target.value)}
              className="flex-1 bg-background border border-border text-xs font-mono px-1.5 py-1 focus:outline-none focus:border-foreground/30"
            >
              <option value="">(agent default)</option>
              {providers.map((p) => (
                <option key={p.provider} value={p.provider}>
                  {p.provider}
                  {!p.available ? ' (not detected)' : ''}
                </option>
              ))}
            </select>
          </Field>
          {selectedUnavailable(provider, providers) && (
            <p className="text-[10px] text-muted-foreground font-mono leading-snug">
              {unavailableHint(provider)}
            </p>
          )}

          {/* Model row */}
          <Field label="model">
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={runnerDefaultModel(provider)}
              className="flex-1 bg-background border border-border text-xs font-mono px-1.5 py-1 focus:outline-none focus:border-foreground/30"
            />
          </Field>

          {/* Effort row */}
          <Field label="effort">
            <div className="flex gap-1">
              {EFFORTS.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setThinkingBudget(opt.value)}
                  className={`text-[11px] font-mono px-2 py-1 border transition-colors ${
                    thinkingBudget === opt.value
                      ? 'bg-primary text-primary-foreground border-primary'
                      : 'bg-background text-muted-foreground border-border hover:border-foreground/20'
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </Field>

          {effortNote(provider, thinkingBudget) && (
            <p className="text-[10px] text-muted-foreground font-mono leading-snug">
              {effortNote(provider, thinkingBudget)}
            </p>
          )}
        </div>
      )}
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex items-center gap-2 text-[11px] font-mono">
      <span className="text-muted-foreground shrink-0 min-w-[70px]">{label}</span>
      {children}
    </label>
  )
}

function selectedUnavailable(
  provider: string,
  providers: { provider: string; available: boolean }[],
): boolean {
  if (!provider) return false
  const p = providers.find((x) => x.provider === provider)
  return p !== undefined && !p.available
}

function unavailableHint(provider: string): string {
  switch (provider) {
    case 'claude':
      return 'Agent could not find `claude` on PATH. It may still work if the agent was launched outside a shell — try it. Otherwise install Claude Code and restart the agent.'
    case 'codex':
      return 'Agent could not find `codex` on PATH. Install OpenAI Codex CLI and restart the agent.'
    case 'anthropic':
      return 'ANTHROPIC_API_KEY not set on the agent. Set it in the env and restart.'
    case 'openai':
      return 'OPENAI_API_KEY not set on the agent. Set it in the env and restart.'
    default:
      return 'This provider is not detected on the agent. It may still work — the probe can be wrong.'
  }
}

/** Hint text shown for each runner's default model. */
function runnerDefaultModel(provider: string): string {
  switch (provider) {
    case 'anthropic':
      return 'claude-opus-4-5'
    case 'openai':
      return 'gpt-4o'
    case 'claude':
      return '(claude CLI picks)'
    case 'codex':
      return '(codex CLI picks)'
    default:
      return '(runner default)'
  }
}

/** Help text when the effort setting may be ignored by the selected runner. */
function effortNote(provider: string, effort: ThinkingEffort): string | null {
  if (effort === 'off') return null
  if (provider === 'openai' || provider === 'codex') {
    return 'Effort is ignored for this runner.'
  }
  if (provider === 'claude') {
    return 'Claude CLI: mapped to a prompt trigger (think/think hard/ultrathink).'
  }
  return null
}
