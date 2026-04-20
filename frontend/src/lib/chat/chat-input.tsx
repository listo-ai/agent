import { useRef, type ReactNode } from 'react'
import { ArrowUp, Loader2, Trash2, Paperclip } from 'lucide-react'
import { useAutoResize } from './hooks'
import { AttachmentPreview } from './attachment-preview'
import type { FileAttachment } from './use-file-attach'

interface Props {
  value: string
  onChange: (value: string) => void
  onSend: () => void
  onKeyDown?: ((e: React.KeyboardEvent) => void) | undefined
  isStreaming: boolean
  placeholder?: string | undefined
  onClear?: (() => void) | undefined
  compact?: boolean | undefined
  accentColor?: string | undefined
  /** Content rendered above the input (e.g. provider selector). */
  header?: ReactNode
  /** Content rendered below the input (e.g. keyboard hints). */
  footer?: ReactNode

  // Attachments
  attachments?: FileAttachment[] | undefined
  onRemoveAttachment?: ((id: string) => void) | undefined
  onOpenFilePicker?: (() => void) | undefined
  fileInputRef?: React.RefObject<HTMLInputElement | null> | undefined
  onFileInputChange?: ((e: React.ChangeEvent<HTMLInputElement>) => void) | undefined
  onDragOver?: ((e: React.DragEvent) => void) | undefined
  onDragLeave?: ((e: React.DragEvent) => void) | undefined
  onDrop?: ((e: React.DragEvent) => void) | undefined
  onPaste?: ((e: React.ClipboardEvent) => void) | undefined
  isDragging?: boolean | undefined
}

export function ChatInput({
  value,
  onChange,
  onSend,
  onKeyDown,
  isStreaming,
  placeholder,
  onClear,
  compact,
  accentColor,
  header,
  footer,
  attachments,
  onRemoveAttachment,
  onOpenFilePicker,
  fileInputRef,
  onFileInputChange,
  onDragOver,
  onDragLeave,
  onDrop,
  onPaste,
  isDragging,
}: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  useAutoResize(textareaRef, value, compact ? 150 : 160)

  const hasAttachments = attachments && attachments.length > 0
  const canSend = (value.trim() || hasAttachments) && !isStreaming

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (onKeyDown) {
      onKeyDown(e)
      if (e.defaultPrevented) return
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      onSend()
    }
  }

  const textSize = compact ? 'text-xs' : 'text-sm'
  const minH = compact ? 'min-h-[20px]' : 'min-h-[24px]'
  const maxH = compact ? 'max-h-[150px]' : 'max-h-[160px]'
  const leading = compact ? 'leading-5' : 'leading-6'
  const iconSize = compact ? 12 : 14
  const btnSize = 'w-7 h-7'

  return (
    <div>
      {header}

      <div
        className={`bg-card border px-3 py-2 focus-within:border-foreground/20 transition-colors ${
          isDragging ? 'border-primary border-dashed bg-primary/5' : 'border-border'
        }`}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
      >
        {hasAttachments && onRemoveAttachment && (
          <AttachmentPreview attachments={attachments!} onRemove={onRemoveAttachment} compact={compact} />
        )}

        {isDragging && (
          <div className="text-center text-xs text-primary py-2 font-mono">Drop files here</div>
        )}

        <div className={`flex items-end ${compact ? 'gap-2' : ''}`}>
          {onOpenFilePicker && (
            <button
              onClick={onOpenFilePicker}
              className={`${btnSize} flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors shrink-0`}
              title="Attach files"
              disabled={isStreaming}
            >
              <Paperclip size={compact ? 12 : 14} />
            </button>
          )}

          <textarea
            ref={textareaRef}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={handleKeyDown}
            onPaste={onPaste}
            placeholder={placeholder || 'Ask anything...'}
            rows={1}
            className={`flex-1 bg-transparent ${textSize} text-foreground placeholder:text-muted-foreground resize-none focus:outline-none ${minH} ${maxH} py-0.5 ${leading}`}
            disabled={isStreaming}
          />
          <div className={`flex items-center gap-1 ${compact ? '' : 'ml-2'}`}>
            {onClear && (
              <button
                onClick={onClear}
                className={`${btnSize} flex items-center justify-center text-muted-foreground hover:text-foreground transition-colors`}
                title="Clear"
              >
                <Trash2 size={compact ? 10 : 14} />
              </button>
            )}
            <button
              onClick={onSend}
              disabled={!canSend}
              className={`${btnSize} flex items-center justify-center disabled:opacity-30 hover:opacity-80 transition-opacity ${
                accentColor ? 'text-white' : 'bg-primary text-primary-foreground'
              }`}
              style={accentColor ? { background: accentColor } : undefined}
            >
              {isStreaming ? (
                <Loader2 size={iconSize} className="animate-spin" />
              ) : (
                <ArrowUp size={iconSize} />
              )}
            </button>
          </div>
        </div>
      </div>

      {fileInputRef && onFileInputChange && (
        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept="image/*,.pdf,.txt,.csv,.md,.json,.yaml,.yml"
          onChange={onFileInputChange}
          className="hidden"
        />
      )}

      {footer}
    </div>
  )
}
