// Pure types for the page builder. No React, no DOM, no browser
// globals — this file is imported by every other layer and by tests
// that run under `vitest run` without jsdom.
//
// `DraftPage` is the builder's single source of truth for one in-progress
// edit. It mirrors what the server stores at `ui.page.layout` but
// carries editor-side bookkeeping: the raw text buffer (so a partially
// invalid JSON document is still editable) and the generation the user
// loaded against.

export interface DraftPage {
  /** ui.page node id (uuid). */
  nodeId: string;
  /** ui.page node path (e.g. `/dashboards/overview`). */
  nodePath: string;
  /**
   * Raw layout JSON text. Kept as a string rather than a parsed object
   * because the editor must allow transient syntax errors during typing.
   */
  layoutText: string;
  /**
   * Slot generation at the time the text was last known-synced with the
   * server. Feeds the OCC guard on save (`writeSlot({expectedGeneration})`).
   */
  baseGeneration: number;
}

/**
 * A single issue produced by either local JSON parsing or the server's
 * `/ui/resolve --dry-run` validator. Shape matches the server's
 * `ResolveIssue` so the two sources compose in one list.
 */
export interface ValidationIssue {
  location: string;
  message: string;
  /** Where the issue came from — useful for later grouping in the UI. */
  source: "parse" | "dry-run";
}
