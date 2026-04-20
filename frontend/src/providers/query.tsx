import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

// One QueryClient instance for the entire app.
// Listed as an MF singleton so remote blocks share this cache.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
    },
  },
});

interface QueryProviderProps {
  children: React.ReactNode;
}

export function QueryProvider({ children }: QueryProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}

/** Expose the shared QueryClient for hooks that need to imperatively invalidate. */
export { queryClient };
