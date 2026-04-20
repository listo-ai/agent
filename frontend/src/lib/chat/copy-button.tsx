import { useState, useCallback } from 'react'
import { Copy, Check } from 'lucide-react'

export function useCopy(ms = 1800) {
  const [copied, setCopied] = useState(false)
  const copy = useCallback(
    (text: string) => {
      void navigator.clipboard.writeText(text).then(() => {
        setCopied(true)
        setTimeout(() => setCopied(false), ms)
      })
    },
    [ms],
  )
  return { copied, copy }
}

export function CopyButton({
  text,
  className = '',
  label = false,
}: {
  text: string
  className?: string
  label?: boolean
}) {
  const { copied, copy } = useCopy()
  return (
    <button
      onClick={() => copy(text)}
      title={copied ? 'Copied!' : 'Copy'}
      className={`flex items-center gap-1 text-[11px] font-mono text-muted-foreground hover:text-foreground transition-colors ${className}`}
    >
      {copied ? <Check size={11} className="text-emerald-400" /> : <Copy size={11} />}
      {label && (copied ? 'copied' : 'copy')}
    </button>
  )
}
