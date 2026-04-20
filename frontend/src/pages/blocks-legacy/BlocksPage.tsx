import { version as hostReactVersion } from "react";
import type { PluginLifecycle, PluginSummary } from "@sys/agent-client";
import { RefreshCw, Eye, EyeOff } from "lucide-react";

import { cn } from "@/lib/utils";
import { useBlocks, usePluginMutations } from "@/hooks/useBlocks";
import { useBlockMount } from "@/hooks/useBlockMount";
import {
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/components/ui";

// ---------------------------------------------------------------------------
// Page component — connects hooks to presentational layer
// ---------------------------------------------------------------------------

export function BlocksPage() {
  const { data, isLoading, isError, error } = useBlocks();
  const { enable, disable, reload } = usePluginMutations();
  const mount = useBlockMount();

  const singletonOk =
    mount.remoteReact !== undefined && mount.remoteReact === hostReactVersion;

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-6">
      <header className="flex items-center gap-3">
        <h1 className="text-base font-semibold">Plugins</h1>
        <span className="text-xs text-muted-foreground">
          Discovered by the agent from its blocks directory.
        </span>
        <Button
          variant="outline"
          size="xs"
          onClick={() => reload.mutate()}
          disabled={reload.isPending}
          className="ml-auto"
          title="POST /api/v1/blocks/reload"
        >
          <RefreshCw size={12} className={cn(reload.isPending && "animate-spin")} />
          {reload.isPending ? "Rescanning…" : "Rescan"}
        </Button>
      </header>

      {isLoading && <PluginsLoadingSkeleton />}

      {isError && (
        <p className="text-sm text-destructive">
          {(error as Error)?.message ?? "failed to load blocks"}
        </p>
      )}

      {data && data.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No blocks found. Drop a block directory under the agent's{" "}
          <code>--blocks-dir</code> and click Rescan.
        </p>
      )}

      {data && data.length > 0 && (
        <ul className="flex flex-col gap-2">
          {data.map((p) => (
            <PluginRow
              key={p.id}
              block={p}
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

      {/* Block UI mount section */}
      {(mount.Panel || mount.error || mount.loading) && (
        <PluginPreview
          Panel={mount.Panel}
          error={mount.error}
          loading={mount.loading}
          remoteReact={mount.remoteReact}
          singletonOk={singletonOk}
          onClose={mount.unmount}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Presentational sub-components
// ---------------------------------------------------------------------------

function PluginsLoadingSkeleton() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 3 }).map((_, i) => (
        <Skeleton key={i} className="h-16 w-full rounded-md" />
      ))}
    </div>
  );
}

interface PluginPreviewProps {
  Panel: React.ComponentType | null;
  error: string | null;
  loading: boolean;
  remoteReact: string | undefined;
  singletonOk: boolean;
  onClose: () => void;
}

function PluginPreview({
  Panel,
  error,
  loading,
  remoteReact,
  singletonOk,
  onClose,
}: PluginPreviewProps) {
  return (
    <Card className="mt-4">
      <CardHeader className="flex-row items-center gap-3">
        <CardTitle className="text-sm">Block preview</CardTitle>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span>
            host React: <code>{hostReactVersion}</code>
          </span>
          {remoteReact && (
            <>
              <span>
                remote React: <code>{remoteReact}</code>
              </span>
              <span className={singletonOk ? "text-emerald-500" : "text-destructive"}>
                {singletonOk ? "singleton ✓" : "MISMATCH — duplicate React!"}
              </span>
            </>
          )}
        </div>
        <Button variant="outline" size="xs" onClick={onClose} className="ml-auto">
          <EyeOff size={12} /> Close
        </Button>
      </CardHeader>
      <CardContent>
        {loading && (
          <p className="text-sm text-muted-foreground">Loading block bundle…</p>
        )}
        {error && <p className="text-sm text-destructive">{error}</p>}
        {Panel && <Panel />}
      </CardContent>
    </Card>
  );
}

interface PluginRowProps {
  block: PluginSummary;
  onEnable: () => void;
  onDisable: () => void;
  onMount: () => void;
  mutating: boolean;
  mountLoading: boolean;
}

function PluginRow({
  block,
  onEnable,
  onDisable,
  onMount,
  mutating,
  mountLoading,
}: PluginRowProps) {
  const disabled = block.lifecycle === "disabled";
  const failed = block.lifecycle === "failed";
  const canMount = block.has_ui && block.lifecycle === "enabled";

  return (
    <li>
      <Card className="py-3">
        <CardContent className="px-4 py-0">
          <div className="flex items-center gap-3">
            <div className="flex flex-col">
              <span className="text-sm font-medium">{block.display_name ?? block.id}</span>
              <span className="text-xs text-muted-foreground">
                {block.id} · v{block.version}
                {block.has_ui && (
                  <>
                    {" · "}
                    <span className="text-muted-foreground">
                      UI: <code>{block.ui_entry}</code>
                    </span>
                  </>
                )}
                {block.kinds.length > 0 && (
                  <>
                    {" · "}
                    <span className="text-muted-foreground">
                      {block.kinds.length} kind{block.kinds.length === 1 ? "" : "s"}
                    </span>
                  </>
                )}
              </span>
            </div>

            <PluginLifecycleBadge lifecycle={block.lifecycle} />

            <div className="ml-auto flex items-center gap-2">
              {canMount && (
                <Button
                  variant="outline"
                  size="xs"
                  onClick={onMount}
                  disabled={mountLoading}
                  title="Load and mount this block's UI panel"
                >
                  <Eye size={12} />
                  View
                </Button>
              )}
              <Button
                variant="outline"
                size="xs"
                onClick={disabled ? onEnable : onDisable}
                disabled={mutating || failed}
                title={
                  failed
                    ? "Block failed to load — fix the manifest and rescan"
                    : undefined
                }
              >
                {disabled ? "Enable" : "Disable"}
              </Button>
            </div>
          </div>

          {block.description && (
            <p className="mt-2 text-xs text-muted-foreground">{block.description}</p>
          )}

          {block.load_errors.length > 0 && (
            <ul className="mt-2 flex flex-col gap-1">
              {block.load_errors.map((err, i) => (
                <li key={i} className="text-xs text-destructive">
                  {err}
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </li>
  );
}

function PluginLifecycleBadge({ lifecycle }: { lifecycle: PluginLifecycle }) {
  const variantMap: Record<PluginLifecycle, "default" | "secondary" | "destructive" | "outline"> = {
    enabled: "default",
    disabled: "secondary",
    failed: "destructive",
    discovered: "outline",
    validated: "outline",
  };
  return (
    <Badge variant={variantMap[lifecycle]} className="text-xs">
      {lifecycle}
    </Badge>
  );
}
