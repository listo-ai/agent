// Pure layout validation. No React, no network.
//
// PR 2 covers local checks only: JSON parse errors (with line/column
// from the parser), plus a shallow structural check on the top-level
// shape. Binding-level validation (`$target.not_a_slot`, unresolved
// `$page` keys) lives on the server's `/ui/resolve --dry-run` and
// lands when the builder gains inline-layout support (PR 3).

import type { ValidationIssue } from "./types.js";

export interface ParsedLayout {
  ok: true;
  value: unknown;
}
export interface ParseFailure {
  ok: false;
  issues: ValidationIssue[];
}
export type ParseResult = ParsedLayout | ParseFailure;

/**
 * Parse the editor buffer and surface structural issues. Line/column
 * pinning for JSON errors uses the parser's reported offset (when the
 * engine provides one) mapped against the source text.
 */
export function validateLayout(text: string): ParseResult {
  if (!text.trim()) {
    return {
      ok: false,
      issues: [
        { source: "parse", location: "root", message: "Layout is empty" },
      ],
    };
  }

  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    const pos = extractErrorOffset(msg);
    const location = pos !== null ? locationFromOffset(text, pos) : "root";
    return {
      ok: false,
      issues: [{ source: "parse", location, message: msg }],
    };
  }

  const structural = checkTopLevel(value);
  if (structural.length > 0) {
    return { ok: false, issues: structural };
  }

  return { ok: true, value };
}

function checkTopLevel(v: unknown): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    issues.push({
      source: "parse",
      location: "root",
      message: "Layout must be a JSON object with `ir_version` and `root`",
    });
    return issues;
  }
  const obj = v as Record<string, unknown>;
  if (typeof obj.ir_version !== "number") {
    issues.push({
      source: "parse",
      location: "root.ir_version",
      message: "`ir_version` must be a number",
    });
  }
  if (typeof obj.root !== "object" || obj.root === null) {
    issues.push({
      source: "parse",
      location: "root.root",
      message: "`root` must be a component object",
    });
    return issues;
  }
  const root = obj.root as Record<string, unknown>;
  if (typeof root.type !== "string") {
    issues.push({
      source: "parse",
      location: "root.root.type",
      message: "Root component must carry a string `type` field",
    });
  }
  return issues;
}

function extractErrorOffset(msg: string): number | null {
  // V8: "Unexpected token } in JSON at position 42"
  // V8 newer: "Expected ',' or ']' after array element in JSON at position 99 (line 3 column 5)"
  const m = /position (\d+)/.exec(msg);
  return m && m[1] ? Number(m[1]) : null;
}

/** `line:col` (1-based), or `root` when the offset is out of range. */
function locationFromOffset(text: string, offset: number): string {
  if (offset < 0 || offset > text.length) return "root";
  let line = 1;
  let col = 1;
  for (let i = 0; i < offset; i++) {
    if (text.charCodeAt(i) === 10) {
      line++;
      col = 1;
    } else {
      col++;
    }
  }
  return `${line}:${col}`;
}
