/**
 * `markdown` component — prose with optional live streaming.
 *
 * Tiny renderer (headings, bold, italics, code, paragraphs) so the
 * SDUI size budget doesn't blow out on a markdown library. Switch to
 * react-markdown / marked if a concrete UC needs tables, math, or
 * GFM task lists.
 */
import { useEffect, useState } from "react";
import { agentPromise } from "@/lib/agent";

type MarkdownNodeShape = {
  type: "markdown";
  id?: string;
  content?: string;
  subscribe?: string;
  mode?: string; // "append" | "replace"
};

export function MarkdownComponent({ node }: { node: MarkdownNodeShape }) {
  const [text, setText] = useState<string>(node.content ?? "");
  const mode = node.mode ?? "append";
  const subject = node.subscribe;

  useEffect(() => {
    setText(node.content ?? "");
  }, [node.content]);

  useEffect(() => {
    if (!subject) return;
    let cancelled = false;
    (async () => {
      const agent = await agentPromise;
      const sub = agent.events.subscribe();
      for await (const e of sub) {
        if (cancelled) break;
        if (e.event !== "slot_changed") continue;
        if (`node.${e.id}.slot.${e.slot}` !== subject) continue;
        const chunk = typeof e.value === "string" ? e.value : JSON.stringify(e.value);
        if (mode === "replace") setText(chunk);
        else setText((prev) => prev + chunk);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [subject, mode]);

  return (
    <div
      className="prose prose-sm max-w-none dark:prose-invert"
      dangerouslySetInnerHTML={{ __html: render(text) }}
    />
  );
}

// Intentionally minimal; escape first, then apply a handful of
// inline patterns. Not security-critical since everything is already
// under the agent's trust boundary, but we escape anyway.
function render(src: string): string {
  const esc = src
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  const lines = esc.split("\n");
  const out: string[] = [];
  let inCode = false;
  for (const ln of lines) {
    if (ln.startsWith("```")) {
      out.push(inCode ? "</code></pre>" : "<pre><code>");
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push(ln);
      continue;
    }
    let h = ln;
    if (/^###\s/.test(h)) h = `<h3>${h.slice(4)}</h3>`;
    else if (/^##\s/.test(h)) h = `<h2>${h.slice(3)}</h2>`;
    else if (/^#\s/.test(h)) h = `<h1>${h.slice(2)}</h1>`;
    else h = `<p>${h}</p>`;
    h = h
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\*([^*]+)\*/g, "<em>$1</em>")
      .replace(/`([^`]+)`/g, "<code>$1</code>");
    out.push(h);
  }
  return out.join("\n");
}
