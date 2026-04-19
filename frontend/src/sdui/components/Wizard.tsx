/**
 * `wizard` component — multi-step form; one step visible at a time,
 * navigates forward/back, fires `submit` on the last step.
 */
import { useState } from "react";
import type { UiAction, UiComponent } from "@sys/agent-client";
import { Renderer } from "../Renderer";
import { useSdui } from "../context";

type WizardStep = { label: string; children: UiComponent[] };
type WizardNodeShape = {
  type: "wizard";
  id?: string;
  steps: WizardStep[];
  submit?: UiAction;
};

export function WizardComponent({ node }: { node: WizardNodeShape }) {
  const { dispatchAction } = useSdui();
  const [idx, setIdx] = useState(0);
  const step = node.steps[idx];
  if (!step) return null;
  const last = idx === node.steps.length - 1;

  return (
    <div className="flex flex-col gap-3 rounded border p-3">
      <div className="flex items-center gap-2 text-sm">
        {node.steps.map((s, i) => (
          <span
            key={i}
            className={`rounded px-2 py-0.5 text-xs ${
              i === idx
                ? "bg-primary text-primary-foreground"
                : "bg-muted text-muted-foreground"
            }`}
          >
            {i + 1}. {s.label}
          </span>
        ))}
      </div>
      <div className="flex flex-col gap-2">
        {step.children.map((c, i) => (
          <Renderer key={(c as { id?: string }).id ?? i} node={c} />
        ))}
      </div>
      <div className="flex justify-end gap-2">
        <button
          type="button"
          className="rounded border px-3 py-1 text-sm disabled:opacity-50"
          disabled={idx === 0}
          onClick={() => setIdx((i) => Math.max(0, i - 1))}
        >
          Back
        </button>
        {last ? (
          <button
            type="button"
            className="rounded bg-primary px-3 py-1 text-sm text-primary-foreground"
            onClick={() => {
              if (node.submit) {
                void dispatchAction(node.submit.handler, node.submit.args);
              }
            }}
          >
            Finish
          </button>
        ) : (
          <button
            type="button"
            className="rounded bg-primary px-3 py-1 text-sm text-primary-foreground"
            onClick={() => setIdx((i) => i + 1)}
          >
            Next
          </button>
        )}
      </div>
    </div>
  );
}
