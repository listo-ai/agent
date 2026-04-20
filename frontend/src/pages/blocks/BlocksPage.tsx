import { useState, version as hostReactVersion, type ComponentType } from "react";
import { useBlocksStore } from "@/store/blocks";
import { loadBlock } from "@/blocks/loader";
import type { BlockManifest } from "@/blocks/types";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui";

// Hard-coded manifest for the dev example block.
// In production, manifests come from the Control Plane (Stage 10 / backend).
const HELLO_MANIFEST: BlockManifest = {
  id: "sys.example.hello",
  name: "Hello Block",
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

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

export function BlocksPage() {
  const blocks = useBlocksStore((s) => [...s.blocks.values()]);
  const [remote, setRemote] = useState<RemoteModule | null>(null);
  const [loading, setLoading] = useState(false);

  async function loadHello() {
    setLoading(true);
    const mod = (await loadBlock(HELLO_MANIFEST)) as RemoteModule | null;
    setRemote(mod);
    setLoading(false);
  }

  const RemotePanel = remote?.default;
  const remoteReact = remote?.REMOTE_REACT_VERSION;
  const singletonOk = remoteReact !== undefined && remoteReact === hostReactVersion;

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <h1 className="text-base font-semibold">Blocks</h1>

      {/* --- MF proof-of-concept panel --- */}
      <MFPocCard
        loading={loading}
        hasRemote={!!remote}
        remoteReact={remoteReact}
        singletonOk={singletonOk}
        onLoad={() => void loadHello()}
        RemotePanel={RemotePanel}
      />

      {/* --- Installed blocks list --- */}
      {blocks.length === 0 ? (
        <p className="text-sm text-muted-foreground">No blocks loaded.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {blocks.map(({ manifest, status, error }) => (
            <li key={manifest.id}>
              <Card className="py-3">
                <CardContent className="px-4 py-0">
                  <div className="flex items-center justify-between">
                    <div className="flex flex-col">
                      <span className="text-sm font-medium">{manifest.name}</span>
                      <span className="text-xs text-muted-foreground">
                        {manifest.id} · v{manifest.version}
                      </span>
                      {error && (
                        <span className="text-xs text-destructive">{error}</span>
                      )}
                    </div>
                    <Badge variant="secondary" className="text-xs">
                      {status}
                    </Badge>
                  </div>
                </CardContent>
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Presentational sub-components
// ---------------------------------------------------------------------------

function MFPocCard({
  loading,
  hasRemote,
  remoteReact,
  singletonOk,
  onLoad,
  RemotePanel,
}: {
  loading: boolean;
  hasRemote: boolean;
  remoteReact: string | undefined;
  singletonOk: boolean;
  onLoad: () => void;
  RemotePanel: ComponentType | undefined;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Module Federation POC</CardTitle>
        <CardDescription>
          Loads <code>@sys/block-hello</code> from{" "}
          <code>http://localhost:3001</code>.
          Run <code>pnpm -F @sys/block-hello dev</code> first.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-3 text-xs">
          <Button size="xs" onClick={onLoad} disabled={loading}>
            {loading ? "Loading…" : hasRemote ? "Reload block" : "Load block"}
          </Button>
          <span>
            host React: <code>{hostReactVersion}</code>
          </span>
          {remoteReact && (
            <>
              <span>
                remote React: <code>{remoteReact}</code>
              </span>
              <span
                className={
                  singletonOk ? "text-emerald-500" : "text-destructive"
                }
              >
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
      </CardContent>
    </Card>
  );
}
