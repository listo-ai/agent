// Shared types for the chat lib. Self-contained — no external hook imports.

export type ChatRole = 'user' | 'assistant' | 'system'

export type MessageStatus =
  | 'connecting'
  | 'thinking'
  | 'tool_call'
  | 'streaming'
  | 'done'
  | 'error'

export interface MessageAttachment {
  name: string
  mimeType: string
  /** base64-encoded data, no data URI prefix */
  data: string
  /** data URI for preview; derived from data+mimeType if omitted */
  previewUrl?: string
}

export interface ChatMessage {
  id: string
  role: ChatRole
  content: string
  /** Tool names invoked by the assistant while producing this message. */
  toolCalls?: string[]
  /** User-attached files (on user messages). */
  attachments?: MessageAttachment[]
  /** Lifecycle status — drives the bubble's spinner / status text. */
  status?: MessageStatus
  /** Wall-clock ms at creation, optional (consumers decide on ordering). */
  timestamp?: number
}
