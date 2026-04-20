import { useEffect, useRef } from 'react'
import { Sparkles, Wrench } from 'lucide-react'

export type PickerItemType = 'prompt' | 'tool'

export interface PickerArg {
  name: string
  required?: boolean
}

export interface PickerItem {
  name: string
  description: string
  type: PickerItemType
  /** Optional category for grouping (e.g. app / server name). */
  group?: string
  arguments?: PickerArg[]
}

interface Props {
  items: PickerItem[]
  /** Items grouped by the `group` field. Pass `{ "": items }` for ungrouped. */
  grouped: Record<string, PickerItem[]>
  selectedIndex: number
  onSelect: (item: PickerItem) => void
}

export function CommandPicker({ items, grouped, selectedIndex, onSelect }: Props) {
  const listRef = useRef<HTMLDivElement>(null)
  const selectedRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  if (items.length === 0) {
    return (
      <div className="absolute bottom-full left-0 right-0 mb-1 border border-border bg-background shadow-2xl max-h-[240px] overflow-y-auto z-50">
        <div className="p-3 text-xs text-muted-foreground text-center">No commands found</div>
      </div>
    )
  }

  let flatIndex = 0

  return (
    <div
      ref={listRef}
      className="absolute bottom-full left-0 right-0 mb-1 border border-border bg-background shadow-2xl max-h-[280px] overflow-y-auto z-50"
    >
      {Object.entries(grouped).map(([groupName, groupItems]) => (
        <div key={groupName || '__ungrouped'}>
          {groupName && (
            <div className="px-3 py-1.5 text-[10px] font-mono uppercase tracking-wider text-muted-foreground/60 bg-muted/30 sticky top-0">
              {groupName}
            </div>
          )}
          {groupItems.map((item) => {
            const thisIndex = flatIndex++
            const isSelected = thisIndex === selectedIndex
            return (
              <button
                key={item.name}
                ref={isSelected ? selectedRef : undefined}
                onClick={() => onSelect(item)}
                className={`w-full text-left px-3 py-2 flex items-start gap-2.5 transition-colors ${
                  isSelected ? 'bg-accent text-accent-foreground' : 'hover:bg-accent/50'
                }`}
              >
                <div className="mt-0.5 shrink-0">
                  {item.type === 'prompt' ? (
                    <Sparkles size={13} className="text-muted-foreground" />
                  ) : (
                    <Wrench size={13} className="text-muted-foreground" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-mono truncate">{item.name}</div>
                  <div className="text-xs text-muted-foreground truncate leading-relaxed">
                    {item.description}
                  </div>
                  {item.arguments && item.arguments.length > 0 && (
                    <div className="flex gap-1 mt-0.5 flex-wrap">
                      {item.arguments.map((a) => (
                        <span
                          key={a.name}
                          className={`text-[9px] font-mono px-1 py-0 border ${
                            a.required
                              ? 'border-primary/40 text-primary'
                              : 'border-border text-muted-foreground'
                          }`}
                        >
                          {a.name}
                          {a.required ? '*' : ''}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </button>
            )
          })}
        </div>
      ))}
    </div>
  )
}
