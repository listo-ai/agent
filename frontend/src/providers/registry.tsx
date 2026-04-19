import { createContext, useContext, useMemo, type ReactNode } from "react";

// Service registry — a Map exposed via React Context.
// Listed as an MF singleton so federated extension modules can call
// useServiceRegistry() and get the same map the host populated.
//
// ~50 LOC, as specified in UI.md.  No InversifyJS.

export type ServiceKey = string;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyService = Record<string, any>;

export type ServiceRegistry = Map<ServiceKey, AnyService>;

const RegistryContext = createContext<ServiceRegistry | null>(null);

export function useServiceRegistry(): ServiceRegistry {
  const ctx = useContext(RegistryContext);
  if (!ctx) throw new Error("useServiceRegistry must be used inside <RegistryProvider>");
  return ctx;
}

export function useService<T extends AnyService>(key: ServiceKey): T {
  const registry = useServiceRegistry();
  const svc = registry.get(key);
  if (!svc) throw new Error(`Service "${key}" not found in registry`);
  return svc as T;
}

interface RegistryProviderProps {
  /** Pre-populate with initial services (e.g. from extension loader). */
  initial?: Record<ServiceKey, AnyService>;
  children: ReactNode;
}

export function RegistryProvider({ initial, children }: RegistryProviderProps) {
  const registry = useMemo<ServiceRegistry>(() => {
    const m = new Map<ServiceKey, AnyService>();
    if (initial) {
      for (const [k, v] of Object.entries(initial)) m.set(k, v);
    }
    return m;
  }, [initial]);

  return (
    <RegistryContext.Provider value={registry}>
      {children}
    </RegistryContext.Provider>
  );
}
