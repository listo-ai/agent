/**
 * ScopeIndicator — shows a subtle banner when Studio is viewing a remote
 * agent scope (i.e. the URL is `/scope/:tenant/:agent_id/...`).
 *
 * Presentational — reads the current scope from `useScope()` and renders:
 *   - Nothing when scope is local.
 *   - A compact "Viewing: <displayName>" strip with a "back to local" link
 *     when scope is remote.
 *
 * Place this at the top of the sidebar content so it's always visible.
 */

import { NavLink } from "react-router-dom";
import { ArrowLeft, Server } from "lucide-react";
import { useScope } from "./useScope";
import { FleetScope } from "@listo/agent-client";

export function ScopeIndicator() {
  const scope = useScope();

  if (FleetScope.isLocal(scope)) return null;

  const label = scope.agent_id;

  return (
    <div className="mx-2 mb-1 flex items-center gap-1.5 rounded-md border border-amber-400/30 bg-amber-50/60 px-2 py-1.5 text-xs dark:bg-amber-950/30">
      <Server size={11} className="shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
      <span className="min-w-0 flex-1 truncate font-medium text-amber-800 dark:text-amber-300">
        {label}
      </span>
      <NavLink
        to="/"
        className="shrink-0 text-amber-600 hover:text-amber-900 dark:text-amber-400 dark:hover:text-amber-100"
        title="Back to local agent"
        aria-label="Back to local agent"
      >
        <ArrowLeft size={11} />
      </NavLink>
    </div>
  );
}
