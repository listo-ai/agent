import { useState } from 'react'
import { BookOpen } from 'lucide-react'

export interface ContextOption {
  name: string
  label: string
}

interface Props {
  options: ContextOption[]
  selected: string[]
  onToggle: (name: string) => void
}

export function ContextPicker({ options, selected, onToggle }: Props) {
  const [open, setOpen] = useState(false)

  return (
    <div className="space-y-1.5">
      <button
        onClick={() => setOpen((p) => !p)}
        className="flex items-center gap-1 text-[10px] font-mono text-muted-foreground hover:text-foreground transition-colors"
      >
        <BookOpen size={10} />
        Context{selected.length > 0 && ` (${selected.length})`}
      </button>
      {open && (
        <div className="flex flex-wrap gap-1">
          {options.map((p) => (
            <button
              key={p.name}
              onClick={() => onToggle(p.name)}
              className={`text-[10px] font-mono px-1.5 py-0.5 border transition-colors ${
                selected.includes(p.name)
                  ? 'bg-primary text-primary-foreground border-primary'
                  : 'bg-background text-muted-foreground border-border hover:border-foreground/20'
              }`}
            >
              {p.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
