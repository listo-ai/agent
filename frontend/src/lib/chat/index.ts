// Visual components
export { ChatBubble } from './chat-bubble'
export { ChatInput } from './chat-input'
export { ChatSuggestions } from './chat-suggestions'
export { Markdown } from './markdown'
export { CopyButton, useCopy } from './copy-button'
export { CommandPicker } from './command-picker'
export { ContextPicker } from './context-picker'
export {
  AttachmentPreview,
  AttachmentInline,
  formatFileSize,
} from './attachment-preview'

// Hooks
export { useAutoScroll, useAutoResize } from './hooks'
export { useFileAttach } from './use-file-attach'
export { useAiChat } from './use-ai-chat'

// Types
export type { ChatMessage, ChatRole, MessageStatus, MessageAttachment } from './types'
export type { FileAttachment, UseFileAttachOpts } from './use-file-attach'
export type { ContextOption } from './context-picker'
export type { PickerItem, PickerItemType, PickerArg } from './command-picker'
export type { UseAiChatOptions } from './use-ai-chat'
