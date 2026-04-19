import { create } from "zustand";
import type { User } from "oidc-client-ts";

// Auth state — populated by AuthProvider from oidc-client-ts events.
// This store is the single source of truth for "is the user logged in?"

interface AuthState {
  user: User | null;
  isAuthenticated: boolean;

  setUser: (user: User) => void;
  clearUser: () => void;
}

export const useAuthStore = create<AuthState>()((set) => ({
  user: null,
  isAuthenticated: false,

  setUser: (user) => set({ user, isAuthenticated: true }),
  clearUser: () => set({ user: null, isAuthenticated: false }),
}));

/** Returns the access token, or null if not authenticated. */
export function getAccessToken(): string | null {
  return useAuthStore.getState().user?.access_token ?? null;
}
