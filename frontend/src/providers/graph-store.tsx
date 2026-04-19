/**
 * GraphStoreProvider — boots one GraphStore per AgentClient lifetime and
 * makes it available to the React tree through GraphStoreContext.
 *
 * Mount this inside AuthProvider (so the token is known) but outside any
 * page components so all pages share the same SSE subscription.
 */

import { useEffect, useState, type ReactNode } from "react";
import { agentPromise } from "@/lib/agent";
import { createGraphStore, GraphStoreContext } from "@/store/graph-hooks";
import type { GraphStore } from "@/store/graph-hooks";

export function GraphStoreProvider({ children }: { children: ReactNode }) {
  const [store, setStore] = useState<GraphStore | null>(null);

  useEffect(() => {
    let mounted = true;
    let created: GraphStore | null = null;

    agentPromise.then((client) => {
      if (!mounted) return;
      created = createGraphStore(client);
      // Zustand stores are functions (UseBoundStore), so passing one directly
      // to setState triggers React's functional-update branch: state = fn(prev).
      // Wrap in an arrow so React sees a factory that returns the store.
      setStore(() => created);
    });

    return () => {
      mounted = false;
      created?.destroy();
    };
  }, []);

  // Render children immediately — hooks will return empty data until the
  // store is ready. This avoids a blank-screen flash while the agent
  // capability handshake completes.
  return (
    <GraphStoreContext.Provider value={store}>
      {children}
    </GraphStoreContext.Provider>
  );
}
