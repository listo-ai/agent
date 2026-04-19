// Composes all providers in the correct order.
// Add new global providers here — keep the nesting shallow.

import type { ReactNode } from "react";
import { QueryProvider } from "./query";
import { AuthProvider } from "./auth";
import { RegistryProvider } from "./registry";

interface ProvidersProps {
  children: ReactNode;
}

export function Providers({ children }: ProvidersProps) {
  return (
    // QueryProvider wraps everything so auth + extension hooks can use useQuery.
    <QueryProvider>
      {/* RegistryProvider exposes the service map as an MF singleton. */}
      <RegistryProvider>
        {/* AuthProvider hydrates user session from OIDC. */}
        <AuthProvider>
          {children}
        </AuthProvider>
      </RegistryProvider>
    </QueryProvider>
  );
}
