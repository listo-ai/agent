import { version as hostReactVersion } from "react";
import type { PluginLifecycle, PluginSummary } from "@sys/agent-client";
import { RefreshCw, Eye, EyeOff } from "lucide-react";

import { cn } from "@/lib/utils";
import { usePlugins, usePluginMutations } from "@/hooks/usePlugins";
import { usePluginMount } from "@/hooks/usePluginMount";

export function PluginsPage() {
  const { data, isLoading, isError, error } = usePlugins();
  const { enable, disable, reload } = usePluginMutations();
  const mount = usePluginMount();

  const singletonOk =
    mount.remoteReact !== undefined && mount.remoteReact === hostReactVersion;

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <header className="flex items-center gap-3">
        <h1 className="text-base font-semibold">Plugins</h1>
        <span className="text-xs text-muted-foreground">
          Discovered by the agent from its plugins directory.
        </span>
        <button
          onClick={() => reload.mutate()}
          disabled={reload.isPending}
          className="ml-auto flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent disabled:opacity-50"
          title="POST /api/v1/plugins/reload"
        >
          <RefreshCw size={12} className={cn(reload.isPending && "animate-spin")} />
          {reload.isPending ? "Rescanning…" : "Rescan"}
        </button>
      </header>

      {isLoading && <p className="text-sm text-muted-foreground">Loading…</p>}

      {isError && (
        <p className="text-sm text-destructive">
          {(error as Error)?.message ?? "failed to load plugins"}
        </p>
      )}

      {data && data.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No plugins found. Drop a plugin directory under the agent's{" "}
          <code>--plugins-dir</code> and click Rescan.
        </p>
      )}

      {data && data.length > 0 && (
        <ul className="flex flex-col gap-2">
          {data.map((p) => (
            <PluginRow
              key={p.id}
              plugin={p}
              onEnable={() => enable.mutate(p.id)}
              onDisable={() => disable.mutate(p.id)}
              onMount={() => mount.mount(p.id)}
              mutating={
                (enable.isPending && enable.variables === p.id) ||
                (disable.isPending && disable.variables === p.id)
              }
              mountLoading={mount.loading}
            />
          ))}
        </ul>
      )}

      {/* Plugin UI mount section */}
      {(mount.Panel || mount.error || mount.loading) && (
        <section className="mt-4 flex flex-col gap-2 rounded-md border border-border p-4">
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-medium">Plugin preview</h2>
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              <span>
                host React: <code>{hostReactVersion}</code>
              </span>
              {mount.remoteReact && (
                <>
                  <span>
                    remote React: <code>{mount.remoteReact}</code>
                  </span>
                  <span className={singletonOk ? "text-emerald-500" : "text-destructive"}>
                    {singletonOk ? "singleton ✓" : "MISMATCH — duplicate React!"}
                  </span>
                </>
              )}
            </div>
            <button
              onClick={mount.unmount}
              className="ml-auto flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
            >
              <EyeOff size={12} /> Close
            </button>
          </div>

          {mount.loading && (
            <p className="text-sm text-muted-foreground">Loading plugin bundle…</p>
          )}
          {mount.error && (
            <p className="text-sm text-destructive">{mount.error}</p>
          )}
          {mount.Panel && (
            <div className="mt-2">
              <mount.Panel />
            </div>
          )}
        </section>
      )}
    </div>
  );
}

interface PluginRowProps {
  plugin: PluginSummary;
  onEnable: () => void;
  onDisable: () => void;
  onMount: () => void;
  mutating: boolean;
  mountLoading: boolean;
}

function PluginRow({
  plugin,
  onEnable,
  onDisable,
  onMount,
  mutating,
  mountLoading,
}: PluginRowProps) {
  const disabled = plugin.lifecycle === "disabled";
  const failed = plugin.lifecycle === "failed";
  const canMount = plugin.has_ui && plugin.lifecycle === "enabled";

  return (
    <li className="rounded-md border border-border px-4 py-3 text-sm">
      <div className="flex items-center gap-3">
        <div className="flex flex-col">
          <span className="font-medium">{plugin.display_name ?? plugin.id}</span>
          <span className="text-xs text-muted-foreground">
            {plugin.id} · v{plugin.version}
            {plugin.has_ui && (
              <>
                {" · "}
                <span className="text-muted-foreground">
                  UI: <code>{plugin.ui_entry}</code>
                </span>
              </>
            )}
            {plugin.kinds.length > 0 && (
              <>
                {" · "}
                <span className="text-muted-foreground">
                  {plugin.kinds.length} kind{plugin.kinds.length === 1 ? "" : "s"}
                </span>
              </>
            )}
          </span>
        </div>

        <LifecycleBadge lifecycle={plugin.lifecycle} />

        <div className="ml-auto flex items-center gap-2">
          {canMount && (
            <button
              onClick={onMount}
              disabled={mountLoading}
              className="flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent disabled:opacity-50"
              title="Load and mount this plugin's UI panel"
            >
              <Eye size={12} />
              View
            </button>
          )}
          <button
            onClick={disabled ? onEnable : onDisable}
            disabled={mutating || failed}
            className="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent disabled:opacity-50"
            title={
              failed
                ? "Plugin failed to load — fix the manifest and rescan"
                : undefined
            }
          >
            {disabled ? "Enable" : "Disable"}
          </button>
        </div>
      </div>

      {plugin.description && (
        <p className="mt-2 text-xs text-muted-foreground">{plugin.description}</p>
      )}

      {plugin.load_errors.length > 0 && (
        <ul className="mt-2 flex flex-col gap-1">
          {plugin.load_errors.map((err, i) => (
            <li key={i} className="text-xs text-destructive">
              {err}
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function LifecycleBadge({ lifecycle }: { lifecycle: PluginLifecycle }) {
  const style: Record<PluginLifecycle, string> = {
    enabled: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
    disabled: "bg-muted text-muted-foreground",
    failed: "bg-destructive/15 text-destructive",
    discovered: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
    validated: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
  };
  return (
    <span
      className={cn(
        "rounded-full px-2 py-0.5 text-xs font-medium",
        style[lifecycle],
      )}
    >
      {lifecycle}
    </span>
  );
}
