/**
 * `timeline` component — chronological event list.
 *
 * With `subscribe` set and `mode: "append"`, incoming NATS messages on
 * the subject are appended locally without a tree re-resolve. This is
 * the UC2 AI-streaming-output + UC3 stage-progress primitive from
 * SDUI.md § "Streaming content". A sentinel `{event: "stream_end"}`
 * terminates the stream; see SDUI.md for lifecycle rules.
 */
import { useEffect, useState } from "react";
import { agentPromise } from "@/lib/agent";

type Event = { ts: string; text: string; intent?: string };
type TimelineNodeShape = {
  type: "timeline";
  id?: string;
  events: Event[];
  subscribe?: string;
  mode?: string; // "append" | "replace"
};

const intentBorder: Record<string, string> = {
  ok: "border-emerald-500",
  warn: "border-amber-500",
  danger: "border-rose-500",
};

export function TimelineComponent({ node }: { node: TimelineNodeShape }) {
  const [live, setLive] = useState<Event[]>(node.events ?? []);
  const mode = node.mode ?? "append";
  const subject = node.subscribe;

  useEffect(() => {
    setLive(node.events ?? []);
  }, [node.events]);

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
        const ev = coerceEvent(e.value);
        if (!ev) continue;
        if (mode === "replace") {
          setLive([ev]);
        } else {
          setLive((prev) => [...prev, ev]);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [subject, mode]);

  if (live.length === 0) {
    return (
      <div className="text-xs text-muted-foreground">No events yet.</div>
    );
  }

  return (
    <ol className="flex flex-col gap-2 text-sm">
      {live.map((e, i) => (
        <li
          key={i}
          className={`rounded border-l-2 pl-3 py-1 ${intentBorder[e.intent ?? ""] ?? "border-muted"}`}
        >
          <div className="text-xs text-muted-foreground">{e.ts}</div>
          <div>{e.text}</div>
        </li>
      ))}
    </ol>
  );
}

function coerceEvent(v: unknown): Event | null {
  if (typeof v === "string") return { ts: new Date().toISOString(), text: v };
  if (v && typeof v === "object") {
    const o = v as Record<string, unknown>;
    const text = typeof o.text === "string" ? o.text : undefined;
    const ts =
      typeof o.ts === "string" ? o.ts : new Date().toISOString();
    if (text) {
      const ev: Event = { ts, text };
      if (typeof o.intent === "string") ev.intent = o.intent;
      return ev;
    }
  }
  return null;
}
