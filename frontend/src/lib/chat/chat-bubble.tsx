import { User, Sparkles, AlertCircle, Wrench, Loader2, Zap } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { CopyButton } from './copy-button'
import { Markdown } from './markdown'
import { AttachmentInline } from './attachment-preview'
import type { ChatMessage, MessageStatus } from './types'

interface Props {
  message: ChatMessage
  isLast: boolean
  isStreaming: boolean
  /** Compact mode — smaller icons/text for embedded panels. */
  compact?: boolean
  /** Custom accent color for the AI icon (CSS color string, e.g. "#6366f1"). */
  accentColor?: string
  /** Transform content before rendering (e.g. strip file blocks). */
  contentTransform?: (content: string) => string
  /** Extra content rendered after the message (e.g. file badges). */
  extraContent?: React.ReactNode
}

export function ChatBubble({
  message,
  isLast,
  isStreaming,
  compact,
  accentColor,
  contentTransform,
  extraContent,
}: Props) {
  const iconSize = compact ? 10 : 12
  const avatarSize = compact ? 'w-5 h-5' : 'w-6 h-6'
  const gap = compact ? 'gap-2' : 'gap-3'

  if (message.role === 'system') {
    return (
      <div className="flex items-start gap-2 px-3 py-2 text-xs text-destructive bg-destructive/5 border border-destructive/10">
        <AlertCircle size={13} className="mt-0.5 shrink-0" />
        <span>{message.content}</span>
      </div>
    )
  }

  if (message.role === 'user') {
    return (
      <div className={`flex items-start ${gap}`}>
        <div
          className={`${avatarSize} rounded-none bg-primary flex items-center justify-center shrink-0 mt-0.5`}
        >
          <User size={iconSize} className="text-primary-foreground" />
        </div>
        <div className="flex-1 min-w-0 pt-0.5">
          <p className={`${compact ? 'text-xs' : 'text-sm'} leading-relaxed text-foreground`}>
            {message.content}
          </p>
          {message.attachments && message.attachments.length > 0 && (
            <AttachmentInline
              attachments={message.attachments.map((a, i) => ({
                id: `msg-att-${i}`,
                name: a.name,
                mimeType: a.mimeType,
                data: a.data,
                previewUrl: a.previewUrl || `data:${a.mimeType};base64,${a.data}`,
                size: 0,
              }))}
              compact={compact}
            />
          )}
        </div>
      </div>
    )
  }

  // Assistant
  const displayText = contentTransform ? contentTransform(message.content) : message.content
  const showCursor = isLast && isStreaming && (message.status === 'streaming' || !!message.content)

  return (
    <div className={`flex items-start ${gap} group`}>
      <div
        className={`${avatarSize} rounded-none flex items-center justify-center shrink-0 mt-0.5`}
        style={{ background: accentColor || 'var(--foreground)' }}
      >
        <Sparkles size={iconSize} className={accentColor ? 'text-white' : 'text-background'} />
      </div>
      <div className="flex-1 min-w-0 space-y-1 pt-0.5">
        {isLast && isStreaming && !message.content && <StatusIndicator status={message.status} />}

        {message.toolCalls && message.toolCalls.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-1">
            {message.toolCalls.map((name, i) => (
              <Badge
                key={i}
                variant="outline"
                className="text-[10px] rounded-none font-mono gap-1 text-muted-foreground"
              >
                <Wrench size={9} /> {name}
              </Badge>
            ))}
          </div>
        )}

        {displayText ? (
          <div className="relative">
            <Markdown content={displayText} compact={compact} />
            {showCursor && (
              <span className="inline-block w-[2px] h-[14px] bg-foreground/50 animate-pulse ml-0.5 align-text-bottom" />
            )}
            {!isStreaming && displayText && (
              <div className="flex justify-end mt-1 opacity-0 group-hover:opacity-100 transition-opacity">
                <CopyButton text={displayText} label={!compact} />
              </div>
            )}
          </div>
        ) : null}

        {!displayText && showCursor && (
          <div className="text-xs">
            <span className="inline-block w-[2px] h-[14px] bg-foreground/60 animate-pulse" />
          </div>
        )}

        {extraContent}
      </div>
    </div>
  )
}

function StatusIndicator({ status }: { status?: MessageStatus | undefined }) {
  switch (status) {
    case 'connecting':
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 size={12} className="animate-spin" />
          <span>Connecting...</span>
        </div>
      )
    case 'thinking':
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 size={12} className="animate-spin" />
          <span>Thinking...</span>
        </div>
      )
    case 'tool_call':
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Zap size={12} className="text-yellow-500" />
          <span>Calling tools...</span>
        </div>
      )
    default:
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 size={12} className="animate-spin" />
          <span>Working...</span>
        </div>
      )
  }
}
