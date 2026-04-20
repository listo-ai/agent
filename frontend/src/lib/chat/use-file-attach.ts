import { useState, useCallback, useRef, type DragEvent, type ClipboardEvent } from 'react'

export interface FileAttachment {
  id: string
  name: string
  mimeType: string
  /** base64-encoded data (no data URI prefix) */
  data: string
  /** data URI for preview */
  previewUrl: string
  size: number
}

const DEFAULT_MAX_FILE_SIZE = 20 * 1024 * 1024 // 20MB
const DEFAULT_ACCEPTED_TYPES = [
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
  'image/svg+xml',
  'application/pdf',
  'text/plain',
  'text/csv',
  'text/markdown',
  'application/json',
  'application/yaml',
]

function generateId() {
  return 'att-' + Math.random().toString(36).slice(2, 10)
}

function fileToAttachment(file: File, maxFileSize: number): Promise<FileAttachment> {
  return new Promise((resolve, reject) => {
    if (file.size > maxFileSize) {
      reject(new Error(`File "${file.name}" exceeds ${maxFileSize / 1024 / 1024}MB limit`))
      return
    }
    const reader = new FileReader()
    reader.onload = () => {
      const dataUri = reader.result as string
      const base64 = dataUri.split(',')[1] || ''
      resolve({
        id: generateId(),
        name: file.name,
        mimeType: file.type || 'application/octet-stream',
        data: base64,
        previewUrl: dataUri,
        size: file.size,
      })
    }
    reader.onerror = () => reject(new Error(`Failed to read "${file.name}"`))
    reader.readAsDataURL(file)
  })
}

export interface UseFileAttachOpts {
  maxFiles?: number
  maxFileSize?: number
  /** MIME types to accept; any `image/*` is always accepted when this is set. */
  acceptedTypes?: string[]
  onError?: (msg: string) => void
}

export function useFileAttach(opts?: UseFileAttachOpts) {
  const maxFiles = opts?.maxFiles ?? 10
  const maxFileSize = opts?.maxFileSize ?? DEFAULT_MAX_FILE_SIZE
  const acceptedTypes = opts?.acceptedTypes ?? DEFAULT_ACCEPTED_TYPES
  const [attachments, setAttachments] = useState<FileAttachment[]>([])
  const [isDragging, setIsDragging] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const addFiles = useCallback(
    async (files: FileList | File[]) => {
      const fileArray = Array.from(files)
      const results: FileAttachment[] = []
      for (const file of fileArray) {
        if (!acceptedTypes.includes(file.type) && !file.type.startsWith('image/')) {
          opts?.onError?.(`Unsupported file type: ${file.type || file.name}`)
          continue
        }
        try {
          const att = await fileToAttachment(file, maxFileSize)
          results.push(att)
        } catch (err) {
          opts?.onError?.((err as Error).message)
        }
      }
      setAttachments((prev) => {
        const combined = [...prev, ...results]
        if (combined.length > maxFiles) {
          opts?.onError?.(`Max ${maxFiles} files allowed`)
          return combined.slice(0, maxFiles)
        }
        return combined
      })
    },
    [acceptedTypes, maxFiles, maxFileSize, opts],
  )

  const remove = useCallback((id: string) => {
    setAttachments((prev) => prev.filter((a) => a.id !== id))
  }, [])

  const clear = useCallback(() => {
    setAttachments([])
  }, [])

  const openFilePicker = useCallback(() => {
    fileInputRef.current?.click()
  }, [])

  const onFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      if (e.target.files && e.target.files.length > 0) {
        void addFiles(e.target.files)
        e.target.value = ''
      }
    },
    [addFiles],
  )

  const onDragOver = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(true)
  }, [])

  const onDragLeave = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(false)
  }, [])

  const onDrop = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(false)
      if (e.dataTransfer.files.length > 0) {
        void addFiles(e.dataTransfer.files)
      }
    },
    [addFiles],
  )

  const onPaste = useCallback(
    (e: ClipboardEvent) => {
      const items = e.clipboardData?.items
      if (!items) return
      const files: File[] = []
      for (let i = 0; i < items.length; i++) {
        const item = items[i]
        if (item?.kind === 'file') {
          const file = item.getAsFile()
          if (file) files.push(file)
        }
      }
      if (files.length > 0) {
        e.preventDefault()
        void addFiles(files)
      }
    },
    [addFiles],
  )

  return {
    attachments,
    isDragging,
    fileInputRef,
    addFiles,
    remove,
    clear,
    openFilePicker,
    onFileInputChange,
    onDragOver,
    onDragLeave,
    onDrop,
    onPaste,
  }
}
