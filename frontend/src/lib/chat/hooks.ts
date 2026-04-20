import { useEffect, type RefObject } from 'react'

/**
 * Auto-scroll a container when content changes.
 * Only scrolls if the user is near the bottom (within threshold),
 * so they can scroll up to read without being yanked back down.
 */
export function useAutoScroll(
  ref: RefObject<HTMLElement | null>,
  deps: unknown[],
  threshold = 100,
) {
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const isNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < threshold
    if (isNearBottom) el.scrollTop = el.scrollHeight
  }, deps) // eslint-disable-line react-hooks/exhaustive-deps
}

/**
 * Auto-resize a textarea to fit its content, up to a max height.
 */
export function useAutoResize(
  ref: RefObject<HTMLTextAreaElement | null>,
  value: string,
  maxHeight = 160,
) {
  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = Math.min(el.scrollHeight, maxHeight) + 'px'
  }, [ref, value, maxHeight])
}
