/**
 * SDUI renderer context — holds the action dispatcher and custom-renderer
 * registry so every component in the tree can reach them without prop-drilling.
 */
import React, { createContext, useContext } from "react";
import type { UiActionResponse } from "@sys/agent-client";

export type ActionFn = (
  handler: string,
  args?: unknown,
) => Promise<UiActionResponse>;

export type CustomRegistry = Map<
  string,
  React.ComponentType<{ props: unknown; subscribe: string[] }>
>;

interface SduiCtx {
  dispatchAction: ActionFn;
  customRegistry: CustomRegistry;
  /** Page-local state (read-only in children; written by set in the page root) */
  pageState: Record<string, unknown>;
  setPageState: (patch: Record<string, unknown>) => void;
}

const SduiContext = createContext<SduiCtx | null>(null);

export function useSdui(): SduiCtx {
  const ctx = useContext(SduiContext);
  if (!ctx) throw new Error("useSdui must be used inside <SduiProvider>");
  return ctx;
}

export function SduiProvider({
  dispatchAction,
  customRegistry,
  pageState,
  setPageState,
  children,
}: SduiCtx & { children: React.ReactNode }) {
  return (
    <SduiContext.Provider
      value={{ dispatchAction, customRegistry, pageState, setPageState }}
    >
      {children}
    </SduiContext.Provider>
  );
}
