/**
 * Platform capability detection.
 * One runtime module — no build-time conditionals for most things.
 * Use platform.isTauri / platform.hasFileSystem etc. in components.
 * Heavy Tauri imports are gated with dynamic import, not here.
 */

const isTauri = "__TAURI_INTERNALS__" in window;

export const platform = {
  /** Running inside a Tauri shell (desktop / mobile). */
  isTauri,
  /** Running as a browser SPA (no Tauri). */
  isBrowser: !isTauri,
  /** True when the File System Access API or Tauri FS is available. */
  hasFileSystem: isTauri || "showDirectoryPicker" in window,
  /** True when native or Web notifications are available. */
  hasNativeNotifications: isTauri || "Notification" in window,
  /** True only on Tauri — can launch / connect to a local agent sidecar. */
  canRunLocalAgent: isTauri,
  /** True only on Tauri — can embed additional web views. */
  canEmbedWebview: isTauri,
} as const;

export type Platform = typeof platform;
