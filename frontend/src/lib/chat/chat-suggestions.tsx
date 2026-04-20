interface Props {
  suggestions: { label: string; prompt: string }[]
  onSelect: (prompt: string) => void
  title?: string
  subtitle?: string
  icon?: React.ReactNode
  compact?: boolean
}

export function ChatSuggestions({
  suggestions,
  onSelect,
  title = 'What can I help with?',
  subtitle,
  icon,
  compact,
}: Props) {
  return (
    <div className={`flex flex-col items-center justify-center h-full px-4 ${compact ? 'py-8' : ''}`}>
      {icon && <div className="mb-3">{icon}</div>}
      <h2 className="font-mono text-lg font-light mb-1 text-foreground">{title}</h2>
      {subtitle && <p className="text-xs text-muted-foreground mb-8">{subtitle}</p>}
      <div
        className={`${
          compact ? 'space-y-2 w-full' : 'flex flex-wrap gap-2 justify-center max-w-lg'
        }`}
      >
        {suggestions.map((s) => (
          <button
            key={s.label}
            onClick={() => onSelect(s.prompt)}
            className={`text-left border border-border bg-card hover:bg-accent transition-colors text-muted-foreground hover:text-foreground ${
              compact ? 'w-full p-2.5 text-[11px] leading-relaxed' : 'px-3 py-2 text-xs'
            }`}
          >
            {s.label}
          </button>
        ))}
      </div>
    </div>
  )
}
