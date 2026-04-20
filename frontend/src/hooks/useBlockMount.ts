import { useCallback, useState, type ComponentType } from "react";
import { registerRemotes, loadRemote } from "@module-federation/enhanced/runtime";

import { AGENT_BASE_URL } from "@/lib/agent";

/**
 * Load a block's MF-exposed `./Panel` module through the agent.
 *
 * The block's container name follows the dot-escape convention from
 * `docs/design/PLUGINS.md` § "Path-segment encoding":
 *
 *   block id    `com.sys.hello`
 *   MF name      `com_acme_hello`
 *
 * So the host can derive the remote name from the block id without
 * extra metadata on the wire.
 */
export function pluginRemoteName(id: string): string {
  return id.replace(/\./g, "_");
}

interface RemoteModule {
  default?: ComponentType;
  REMOTE_REACT_VERSION?: string;
}

export interface PluginMountState {
  Panel: ComponentType | null;
  remoteReact: string | undefined;
  loading: boolean;
  error: string | null;
  mount: (pluginId: string) => Promise<void>;
  unmount: () => void;
}

export function useBlockMount(): PluginMountState {
  const [Panel, setPanel] = useState<ComponentType | null>(null);
  const [remoteReact, setRemoteReact] = useState<string | undefined>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const mount = useCallback(async (pluginId: string) => {
    setLoading(true);
    setError(null);
    setPanel(null);
    setRemoteReact(undefined);
    try {
      const name = pluginRemoteName(pluginId);
      // Agent serves the block's ui/ at `/blocks/<id>/…`. MF manifest
      // → reliable MF handshake; remoteEntry.js works too.
      const entry = `${AGENT_BASE_URL}/blocks/${encodeURIComponent(pluginId)}/mf-manifest.json`;
      registerRemotes([{ name, entry }], { force: true });

      const mod = await loadRemote<RemoteModule>(`${name}/Panel`);
      if (!mod?.default) {
        throw new Error(`remote ${name}/Panel returned no default export`);
      }
      setPanel(() => mod.default!);
      setRemoteReact(mod.REMOTE_REACT_VERSION);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const unmount = useCallback(() => {
    setPanel(null);
    setRemoteReact(undefined);
    setError(null);
  }, []);

  return { Panel, remoteReact, loading, error, mount, unmount };
}
