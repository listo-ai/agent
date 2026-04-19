import { useNodes } from "@/hooks/useAgent";
import { AGENT_BASE_URL } from "@/lib/agent";

/**
 * Flows page — first real wire-up to the live agent REST surface via
 * `@acme/agent-client`. Intentionally minimal: one GET against
 * `/api/v1/nodes` and a plain list render. Milestone 5 replaces this
 * with the React Flow canvas + property panel; until then this page
 * serves as the "end-to-end studio talks to agent" smoke test.
 */
export function FlowsPage() {
  const nodes = useNodes();

  if (nodes.isPending) {
    return <CenteredMessage title="Loading nodes…" detail={AGENT_BASE_URL} />;
  }

  if (nodes.isError) {
    return (
      <CenteredMessage
        title="Could not reach the agent"
        detail={`${AGENT_BASE_URL} — ${formatError(nodes.error)}`}
      />
    );
  }

  const list = nodes.data;
  if (list.length === 0) {
    return (
      <CenteredMessage
        title="No nodes yet"
        detail={`Connected to ${AGENT_BASE_URL}. Seed a flow via POST /api/v1/seed.`}
      />
    );
  }

  const sorted = [...list].sort((a, b) => a.path.localeCompare(b.path));

  return (
    <div className="flex h-full flex-col gap-3 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-lg font-semibold">Graph</h1>
        <span className="text-xs text-muted-foreground">
          {sorted.length} node{sorted.length === 1 ? "" : "s"} · {AGENT_BASE_URL}
        </span>
      </header>
      <ul className="divide-y divide-border rounded-md border border-border bg-background">
        {sorted.map((n) => (
          <li key={n.id} className="flex items-baseline justify-between px-3 py-2 text-sm">
            <span className="font-mono">{n.path}</span>
            <span className="flex gap-3 text-xs text-muted-foreground">
              <span className="font-mono">{n.kind}</span>
              <span>{n.lifecycle}</span>
              <span>
                {n.slots.length} slot{n.slots.length === 1 ? "" : "s"}
              </span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function CenteredMessage({ title, detail }: { title: string; detail: string }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
      <p className="text-sm">{title}</p>
      <p className="text-xs font-mono">{detail}</p>
    </div>
  );
}

function formatError(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as { message: unknown }).message);
  }
  return String(err);
}
