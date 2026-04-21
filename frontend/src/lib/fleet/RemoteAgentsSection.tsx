/**
 * RemoteAgentsSection — sidebar section listing known remote agents.
 *
 * Presentational component. Receives `RemoteAgentNode[]` as props so it
 * is testable and reusable without a GraphStore in context.
 *
 * Clicking a remote-agent item navigates to `/scope/:tenant/:agent_id/`
 * which pushes the scope into the URL — the URL becomes the source of
 * truth per FLEET-TRANSPORT.md § "How Studio descends into a remote".
 */

import { NavLink } from "react-router-dom";
import { Server, Wifi, WifiOff, Loader2, Radio } from "lucide-react";
import {
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
} from "@listo/ui-kit";
import type { RemoteAgentNode, RemoteAgentConnection } from "./types";

// ---------------------------------------------------------------------------
// Connection badge
// ---------------------------------------------------------------------------

interface ConnectionBadgeProps {
  connection: RemoteAgentConnection;
}

function ConnectionBadge({ connection }: ConnectionBadgeProps) {
  switch (connection) {
    case "connected":
      return (
        <Wifi
          size={11}
          className="ml-auto shrink-0 text-green-500"
          aria-label="Connected"
        />
      );
    case "reconnecting":
      return (
        <Loader2
          size={11}
          className="ml-auto shrink-0 animate-spin text-yellow-500"
          aria-label="Reconnecting"
        />
      );
    case "disconnected":
      return (
        <WifiOff
          size={11}
          className="ml-auto shrink-0 text-red-400"
          aria-label="Disconnected"
        />
      );
    default:
      return (
        <Radio
          size={11}
          className="ml-auto shrink-0 text-muted-foreground/50"
          aria-label="Unknown"
        />
      );
  }
}

// ---------------------------------------------------------------------------
// RemoteAgentItem — one sidebar row
// ---------------------------------------------------------------------------

interface RemoteAgentItemProps {
  agent: RemoteAgentNode;
}

function RemoteAgentItem({ agent }: RemoteAgentItemProps) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild tooltip={`${agent.tenant} / ${agent.agentId}`}>
        <NavLink to={agent.scopeUrl}>
          <Server size={14} className="shrink-0" aria-hidden />
          <span className="truncate">{agent.displayName}</span>
          <ConnectionBadge connection={agent.connection} />
        </NavLink>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

// ---------------------------------------------------------------------------
// RemoteAgentsSection — the list
// ---------------------------------------------------------------------------

export interface RemoteAgentsSectionProps {
  agents: RemoteAgentNode[];
}

/**
 * Renders the list of remote agents. Returns `null` when `agents` is empty
 * so the caller can choose whether to show an empty state or nothing at all.
 */
export function RemoteAgentsSection({ agents }: RemoteAgentsSectionProps) {
  if (agents.length === 0) return null;

  return (
    <SidebarMenu>
      {agents.map((agent) => (
        <RemoteAgentItem key={agent.id} agent={agent} />
      ))}
    </SidebarMenu>
  );
}
