// Monaco-backed editor for `ui.page.layout`. Controlled by the
// builder store: value reads from `draft.layoutText`, every edit
// pushes straight back through `setLayoutText`. Markers are projected
// from `store.issues` so the validator (`persistence/use-validator`)
// and the squigglies stay in lockstep.
//
// Monaco's built-in JSON language service gives us brace matching,
// bracket colourisation, and structural errors for free. A schema
// reference (pulled from `/api/v1/ui/vocabulary`) will layer on top
// in a follow-up; until then, semantic validation flows through the
// builder store instead of Monaco's own model markers so there is
// exactly one source of truth.

import { useEffect, useRef } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor as MonacoEditor } from "monaco-editor";
import { useBuilderStore } from "../store/builder-store";
import type { ValidationIssue } from "../model/types";

const OWNER = "page-builder";

export function EditorPane() {
  const draft = useBuilderStore((s) => s.draft);
  const issues = useBuilderStore((s) => s.issues);
  const setLayoutText = useBuilderStore((s) => s.setLayoutText);

  const editorRef = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof import("monaco-editor") | null>(null);

  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
  };

  // Project store issues onto Monaco model markers. Runs on every
  // issues change; no debouncing needed — issues are already debounced
  // upstream by the validator.
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    const model = ed.getModel();
    if (!model) return;
    monaco.editor.setModelMarkers(
      model,
      OWNER,
      issues.map((issue) => toMarker(model, issue, monaco)),
    );
  }, [issues]);

  if (!draft) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
        Loading…
      </div>
    );
  }

  return (
    <Editor
      height="100%"
      language="json"
      value={draft.layoutText}
      onChange={(v) => setLayoutText(v ?? "")}
      onMount={handleMount}
      options={{
        minimap: { enabled: false },
        fontSize: 12,
        tabSize: 2,
        wordWrap: "on",
        scrollBeyondLastLine: false,
        automaticLayout: true,
        lineNumbers: "on",
        renderWhitespace: "selection",
      }}
    />
  );
}

function toMarker(
  model: MonacoEditor.ITextModel,
  issue: ValidationIssue,
  monaco: typeof import("monaco-editor"),
): MonacoEditor.IMarkerData {
  const { line, col } = parseLineCol(issue.location) ?? { line: 1, col: 1 };
  const wordAt = model.getWordAtPosition({ lineNumber: line, column: col });
  return {
    severity: monaco.MarkerSeverity.Error,
    startLineNumber: line,
    startColumn: wordAt ? wordAt.startColumn : col,
    endLineNumber: line,
    endColumn: wordAt ? wordAt.endColumn : Math.max(col + 1, col),
    message: issue.message,
    source: issue.source,
  };
}

/** Parses `"12:3"` location strings. Non-matching returns `null`. */
function parseLineCol(s: string): { line: number; col: number } | null {
  const m = /^(\d+):(\d+)$/.exec(s);
  if (!m || !m[1] || !m[2]) return null;
  return { line: Number(m[1]), col: Number(m[2]) };
}
