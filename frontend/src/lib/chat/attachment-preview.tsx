import { X, FileText, Image as ImageIcon } from 'lucide-react'
import type { FileAttachment } from './use-file-attach'

interface Props {
  attachments: FileAttachment[]
  onRemove: (id: string) => void
  compact?: boolean | undefined
}

export function AttachmentPreview({ attachments, onRemove, compact }: Props) {
  if (attachments.length === 0) return null

  const size = compact ? 48 : 56

  return (
    <div className="flex gap-1.5 flex-wrap py-1.5">
      {attachments.map((att) => (
        <div
          key={att.id}
          className="relative group border border-border bg-card overflow-hidden"
          style={{ width: size, height: size }}
        >
          {att.mimeType.startsWith('image/') ? (
            <img src={att.previewUrl} alt={att.name} className="w-full h-full object-cover" />
          ) : (
            <div className="w-full h-full flex flex-col items-center justify-center gap-0.5 px-1">
              {att.mimeType === 'application/pdf' ? (
                <FileText size={14} className="text-red-400" />
              ) : (
                <ImageIcon size={14} className="text-muted-foreground" />
              )}
              <span className="text-[8px] text-muted-foreground truncate w-full text-center font-mono">
                {att.name.length > 10 ? att.name.slice(0, 8) + '...' : att.name}
              </span>
            </div>
          )}

          <button
            onClick={() => onRemove(att.id)}
            className="absolute top-0 right-0 w-4 h-4 bg-black/60 text-white flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
          >
            <X size={10} />
          </button>
        </div>
      ))}
    </div>
  )
}

/** Render attachments inline in a chat message (read-only, no remove). */
export function AttachmentInline({
  attachments,
  compact,
}: {
  attachments: FileAttachment[]
  compact?: boolean | undefined
}) {
  if (attachments.length === 0) return null

  const size = compact ? 80 : 120

  return (
    <div className="flex gap-1.5 flex-wrap my-1">
      {attachments.map((att) => (
        <div key={att.id} className="border border-border overflow-hidden" style={{ maxWidth: size }}>
          {att.mimeType.startsWith('image/') ? (
            <img src={att.previewUrl} alt={att.name} className="w-full h-auto" />
          ) : (
            <div className="flex items-center gap-1 px-2 py-1">
              <FileText size={12} className="text-muted-foreground shrink-0" />
              <span className="text-[10px] font-mono text-muted-foreground truncate">{att.name}</span>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

/** Format file size for display. */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return bytes + 'B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + 'KB'
  return (bytes / (1024 * 1024)).toFixed(1) + 'MB'
}
