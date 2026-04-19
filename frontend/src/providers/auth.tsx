import { createContext, useContext, useEffect, type ReactNode } from "react";
import { UserManager, type User } from "oidc-client-ts";
import { useAuthStore } from "@/store/auth";

// OIDC settings — override via PUBLIC_* env vars (Rsbuild convention).
const env = (import.meta as unknown as { env: Record<string, string | undefined> }).env;
const oidcSettings = {
  authority: env["PUBLIC_ZITADEL_URL"] ?? "http://localhost:8080",
  client_id: env["PUBLIC_CLIENT_ID"] ?? "studio",
  redirect_uri: `${window.location.origin}/auth/callback`,
  scope: "openid profile email",
  automaticSilentRenew: true,
} as const;

// Single UserManager for the app lifetime.
export const userManager = new UserManager(oidcSettings);

interface AuthContextValue {
  login: () => Promise<void>;
  logout: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}

interface AuthProviderProps {
  children: ReactNode;
}

export function AuthProvider({ children }: AuthProviderProps) {
  const setUser = useAuthStore((s) => s.setUser);
  const clearUser = useAuthStore((s) => s.clearUser);

  useEffect(() => {
    // Hydrate from existing session on mount.
    void userManager.getUser().then((u: User | null) => {
      if (u && !u.expired) setUser(u);
    });

    const onUserLoaded    = (u: User) => setUser(u);
    const onUserUnloaded  = ()        => clearUser();

    userManager.events.addUserLoaded(onUserLoaded);
    userManager.events.addUserUnloaded(onUserUnloaded);
    userManager.events.addAccessTokenExpired(onUserUnloaded);

    return () => {
      userManager.events.removeUserLoaded(onUserLoaded);
      userManager.events.removeUserUnloaded(onUserUnloaded);
      userManager.events.removeAccessTokenExpired(onUserUnloaded);
    };
  }, [setUser, clearUser]);

  const value: AuthContextValue = {
    login:  () => userManager.signinRedirect(),
    logout: () => userManager.signoutRedirect(),
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}
