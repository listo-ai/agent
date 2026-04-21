import { useState } from "react";
import { cn } from "@/lib/utils";
import { Button } from "@listo/ui-kit";
import { Card, CardContent } from "@listo/ui-kit";
import { Input } from "@listo/ui-kit";
import { Label } from "@listo/ui-kit";
import { Separator } from "@listo/ui-kit";

interface LoginFormProps extends React.ComponentProps<"div"> {}

export function LoginForm({ className, ...props }: LoginFormProps) {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setIsLoading(true);
    try {
      // TODO: wire up auth when ready
      await new Promise((resolve) => setTimeout(resolve, 500));
    } catch {
      setError("Invalid email or password.");
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <Card className="overflow-hidden p-0">
        <CardContent className="grid p-0 md:grid-cols-2">
          {/* Left — form */}
          <form className="p-6 md:p-8" onSubmit={(e) => void handleSubmit(e)}>
            <div className="flex flex-col gap-6">
              <div className="flex flex-col items-center gap-2 text-center">
                <h1 className="text-2xl font-bold">Welcome back</h1>
                <p className="text-balance text-sm text-muted-foreground">
                  Sign in to your Studio account
                </p>
              </div>

              {error && (
                <p className="rounded-md bg-destructive/10 px-3 py-2 text-center text-sm text-destructive">
                  {error}
                </p>
              )}

              <div className="flex flex-col gap-4">
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="email">Email</Label>
                  <Input
                    id="email"
                    type="email"
                    placeholder="you@example.com"
                    autoComplete="email"
                    required
                    disabled={isLoading}
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                  />
                </div>

                <div className="flex flex-col gap-1.5">
                  <div className="flex items-center">
                    <Label htmlFor="password">Password</Label>
                    <a
                      href="#"
                      className="ml-auto text-sm text-muted-foreground underline-offset-4 hover:underline"
                    >
                      Forgot password?
                    </a>
                  </div>
                  <Input
                    id="password"
                    type="password"
                    autoComplete="current-password"
                    required
                    disabled={isLoading}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </div>

                <Button type="submit" className="w-full" disabled={isLoading}>
                  {isLoading ? "Signing in…" : "Sign in"}
                </Button>
              </div>

              <div className="relative">
                <Separator />
                <span className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 bg-card px-2 text-xs text-muted-foreground">
                  Or continue with
                </span>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <Button variant="outline" type="button" disabled={isLoading}>
                  {/* Google */}
                  <svg className="mr-2 h-4 w-4" viewBox="0 0 24 24" aria-hidden>
                    <path
                      d="M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z"
                      fill="currentColor"
                    />
                  </svg>
                  Google
                </Button>
                <Button variant="outline" type="button" disabled={isLoading}>
                  {/* GitHub */}
                  <svg className="mr-2 h-4 w-4" viewBox="0 0 24 24" aria-hidden>
                    <path
                      d="M12 0C5.37 0 0 5.37 0 12c0 5.302 3.438 9.8 8.207 11.385.6.113.82-.258.82-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23A11.509 11.509 0 0 1 12 5.803c1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.218.694.825.576C20.565 21.795 24 17.298 24 12c0-6.63-5.37-12-12-12z"
                      fill="currentColor"
                    />
                  </svg>
                  GitHub
                </Button>
              </div>

              <p className="text-center text-sm text-muted-foreground">
                Don&apos;t have an account?{" "}
                <a
                  href="#"
                  className="font-medium text-foreground underline-offset-4 hover:underline"
                >
                  Sign up
                </a>
              </p>
            </div>
          </form>

          {/* Right — decorative panel */}
          <div className="relative hidden items-center justify-center bg-muted md:flex">
            <div className="flex flex-col items-center gap-4 px-8 text-center">
              <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-foreground text-background">
                <svg viewBox="0 0 24 24" className="h-7 w-7" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
                  <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </div>
              <div>
                <p className="text-lg font-semibold">Studio</p>
                <p className="mt-1 text-sm text-muted-foreground">
                  The visual editor for your agent network.
                </p>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      <p className="px-6 text-center text-xs text-muted-foreground">
        By signing in you agree to our{" "}
        <a href="#" className="underline underline-offset-4 hover:text-foreground">
          Terms of Service
        </a>{" "}
        and{" "}
        <a href="#" className="underline underline-offset-4 hover:text-foreground">
          Privacy Policy
        </a>
        .
      </p>
    </div>
  );
}
