import { useState, version as hostReactVersion, type ComponentType } from "react";
import { useExtensionsStore } from "@/store/extensions";
import { loadExtension } from "@/extensions/loader";
import type { ExtensionManifest } from "@/extensions/types";

// Hard-coded manifest for the dev example plugin.
// In production, manifests come from the Control Plane (Stage 10 / backend).
const HELLO_MANIFEST: ExtensionManifest = {
  id: "sys.example.hello",
  name: "Hello Plugin",
  version: "0.1.0",
  trust: "first-party",
  remoteEntry: "http://localhost:3001/mf-manifest.json",
  remoteName: "plugin_hello",
  exposedModule: "./Panel",
  contributions: {},
};

type RemoteModule = {
  default?: ComponentType;
  REMOTE_REACT_VERSION?: string;
};

export function ExtensionsPage() {
  const extensions = useExtensionsStore((s) => [...s.extensions.values()]);
  const [remote, setRemote] = useState<RemoteModule | null>(null);
  const [loading, setLoading] = useState(false);

  async function loadHello() {
    setLoading(true);
    const mod = (await loadExtension(HELLO_MANIFEST)) as RemoteModule | null;
    setRemote(mod);
    setLoading(false);
  }

  const RemotePanel = remote?.default;
  const remoteReact = remote?.REMOTE_REACT_VERSION;
  const singletonOk = remoteReact !== undefined && remoteReact === hostReactVersion;

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <h1 className="text-base font-semibold">Extensions</h1>

      {/* --- MF proof-of-concept panel --- */}
      <section className="rounded-md border border-border p-4">
        <h2 className="text-sm font-medium">Module Federation POC</h2>
        <p className="text-xs text-muted-foreground">
          Loads <code>@sys/plugin-hello</code> from <code>http://localhost:3001</code>.
          Run <code>pnpm -F @sys/plugin-hello dev</code> first.
        </p>
        <div className="mt-3 flex items-center gap-3 text-xs">
          <button
            onClick={() => void loadHello()}
            disabled={loading}
            className="rounded-md bg-primary px-3 py-1 font-medium text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {loading ? "Loading…" : remote ? "Reload plugin" : "Load plugin"}
          </button>
          <span>host React: <code>{hostReactVersion}</code></span>
          {remoteReact && (
            <>
              <span>remote React: <code>{remoteReact}</code></span>
              <span className={singletonOk ? "text-emerald-500" : "text-destructive"}>
                {singletonOk ? "singleton ✓" : "MISMATCH — duplicate React!"}
              </span>
            </>
          )}
        </div>

        {RemotePanel && (
          <div className="mt-4">
            <RemotePanel />
          </div>
        )}
      </section>

      {/* --- Installed extensions list --- */}
      {extensions.length === 0 ? (
        <p className="text-sm text-muted-foreground">No extensions loaded.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {extensions.map(({ manifest, status, error }) => (
            <li
              key={manifest.id}
              className="flex items-center justify-between rounded-md border border-border px-4 py-3 text-sm"
            >
              <div className="flex flex-col">
                <span className="font-medium">{manifest.name}</span>
                <span className="text-xs text-muted-foreground">
                  {manifest.id} · v{manifest.version}
                </span>
                {error && <span className="text-xs text-destructive">{error}</span>}
              </div>
              <span className="text-xs text-muted-foreground">{status}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
