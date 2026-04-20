/**
 * Watches react-router's location and keeps the global chat store's
 * `context` field in sync. Mounted once inside [`Shell`] so every
 * navigation updates the context immediately.
 */
import { useEffect } from 'react'
import { useLocation } from 'react-router-dom'
import { parseRoute } from './context'
import { useGlobalAiChat } from './store'

export function useChatContextSync() {
  const loc = useLocation()
  const setContext = useGlobalAiChat((s) => s.setContext)

  useEffect(() => {
    setContext(parseRoute(loc.pathname))
  }, [loc.pathname, setContext])
}
