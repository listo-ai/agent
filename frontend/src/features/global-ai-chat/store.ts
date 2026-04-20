/**
 * Global AI chat UI state.
 *
 * Splits into:
 *   - ephemeral: `open`, `context` — recomputed from the route every
 *     navigation; never persisted.
 *   - persisted: `extraHints`, `provider`, `model`, `thinkingBudget` —
 *     survive a reload. Persisted under the `global-ai-chat` key so
 *     it's easy to clear from devtools if it gets wedged.
 */
import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { ChatContext } from './context'

export type ThinkingEffort = 'off' | 'low' | 'medium' | 'high'

interface PersistedState {
  extraHints: string
  /** Provider id (`anthropic` | `openai` | `claude` | `codex`); `''` = agent default. */
  provider: string
  /** Model override; `''` = runner default. */
  model: string
  /** Reasoning effort / thinking budget alias. */
  thinkingBudget: ThinkingEffort
}

interface GlobalAiChatState extends PersistedState {
  open: boolean
  context: ChatContext

  setOpen: (open: boolean) => void
  toggle: () => void
  setContext: (context: ChatContext) => void
  setExtraHints: (hints: string) => void
  setProvider: (provider: string) => void
  setModel: (model: string) => void
  setThinkingBudget: (budget: ThinkingEffort) => void
}

export const useGlobalAiChat = create<GlobalAiChatState>()(
  persist(
    (set) => ({
      open: false,
      context: { kind: 'unknown', path: '/' },
      extraHints: '',
      provider: '',
      model: '',
      thinkingBudget: 'off',

      setOpen: (open) => set({ open }),
      toggle: () => set((s) => ({ open: !s.open })),
      setContext: (context) => set({ context }),
      setExtraHints: (extraHints) => set({ extraHints }),
      setProvider: (provider) => set({ provider }),
      setModel: (model) => set({ model }),
      setThinkingBudget: (thinkingBudget) => set({ thinkingBudget }),
    }),
    {
      name: 'global-ai-chat',
      partialize: (state): PersistedState => ({
        extraHints: state.extraHints,
        provider: state.provider,
        model: state.model,
        thinkingBudget: state.thinkingBudget,
      }),
    },
  ),
)
