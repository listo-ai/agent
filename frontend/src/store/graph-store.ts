/**
 * GraphStore — the single source of truth for the Studio frontend.
 *
 * Both the sidebar tree and the flow canvas read from this path-keyed
 * node cache. It subscribes to the SSE stream once and fans the events
 * out to every consumer. See REACTIVE-UI.md for the full design.
 *
 * Usage
 * -----
 *   const store = createGraphStore(agentClient);
 *   // Wrap in <GraphStoreContext.Provider value={store}>
 *
 * The store is created once per AgentClient instance: one SSE
 * connection, one cache, many consumer components.
 */

import { create } from "zustand";
import type { AgentClient, GraphEvent, Link, NodeSnapshot } from "@sys/agent-client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/** A pending optimistic slot write — cleared when either signal arrives. */
export interface PendingWrite {
  /** Generation we expect the server to confirm. */
  expectedGen: number;
  /** Value we wrote optimistically into the cache. */
  value: unknown;
  /** Pre-write value — used for conflict revert. */
  prevValue: unknown;
  /** Previous generation — used for conflict revert. */
  prevGen: number;
  startedAt: number;
}

export interface GraphStoreState {
  // ----- Cache -----
  nodes: Map<string, NodeSnapshot>;
  links: Map<string, Link>;

  // ----- Navigation (per-session, never synced to server) -----
  /** Paths that are expanded in the sidebar tree. */
  expanded: Set<string>;
  /** Path of the flow node currently open on the canvas. */
  openFlow: string | null;
  /** Path of the node currently open in the property panel. */
  selection: string | null;

  // ----- Optimistic writes -----
  /** Key is `${path}::${slot}`. At most one per (path, slot). */
  pending: Map<string, PendingWrite>;
  /** Conflict map: key is `${path}::${slot}`, value is server value. */
  conflicts: Map<string, unknown>;

  // ----- SSE -----
  lastSeq: number;

  // ----- Loading state -----
  loadingPaths: Set<string>;

  // ----- Actions -----
  /**
   * Expand a path in the sidebar. Triggers a lazy child fetch if the
   * children for this path haven't been loaded yet.
   */
  expand: (path: string) => void;

  /** Collapse a path. Cache is retained (collapse is cosmetic). */
  collapse: (path: string) => void;

  /**
   * Open a flow on the canvas. Triggers a subtree + link fetch if not
   * already loaded.
   */
  setOpenFlow: (path: string | null) => void;

  /** Select a node for the property panel. */
  setSelection: (path: string | null) => void;

  /**
   * Optimistic slot write. Updates the cache immediately, then waits
   * for either the `SlotChanged` SSE event or the HTTP response to
   * confirm or revert.
   */
  writeSlot: (path: string, slot: string, value: unknown) => Promise<void>;

  /**
   * Apply a graph event from the SSE stream to the cache. Idempotent —
   * applying the same event twice yields the same state.
   */
  applyEvent: (event: GraphEvent) => void;

  /**
   * Reconcile after a reconnect or `cursor_too_old`.  Refetches every
   * expanded branch and the open flow subtree.
   */
  reconcile: () => void;

  /** Internal: merge fetched node snapshots into the cache. */
  _mergeNodes: (snapshots: NodeSnapshot[]) => void;

  /** Internal: merge fetched links into the link cache. */
  _mergeLinks: (links: Link[]) => void;
}

// ---------------------------------------------------------------------------
// Batched NodeCreated fetch state (outside store, per-instance)
// ---------------------------------------------------------------------------
interface PendingFetchEntry {
  id: string;
  path: string;
  parentPath: string | null;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/**
 * Create a GraphStore bound to the given agent client.
 *
 * The store opens one SSE subscription using the subscribe-before-fetch
 * protocol:
 *   1. Open SSE → receive `hello { seq }` → set `lastSeq`
 *   2. Queue arriving events
 *   3. Fetch initial visible data (root children if any paths are in
 *      `expanded`, the open flow subtree if `openFlow` is set)
 *   4. Drain the queued events into the cache
 *   5. Subsequent events apply directly
 */
export function createGraphStore(client: AgentClient) {
  // Event queue: events arriving before initial fetch is complete
  const earlyQueue: GraphEvent[] = [];
  let initialFetchDone = false;

  // Batched NodeCreated debounce state
  const pendingFetches = new Map<string, PendingFetchEntry>();
  let fetchTimer: ReturnType<typeof setTimeout> | null = null;

  const store = create<GraphStoreState>()((set, get) => ({
    nodes: new Map(),
    links: new Map(),
    expanded: new Set(),
    openFlow: null,
    selection: null,
    pending: new Map(),
    conflicts: new Map(),
    lastSeq: 0,
    loadingPaths: new Set(),

    // ----------------------------------------------------------------
    // expand / collapse
    // ----------------------------------------------------------------

    expand(path) {
      set((s) => {
        const expanded = new Set(s.expanded);
        expanded.add(path);
        return { expanded };
      });

      // Lazy fetch: load children if not yet in cache
      const { nodes, loadingPaths } = get();
      const alreadyHaveChildren = [...nodes.values()].some(
        (n) => n.parent_path === path,
      );
      const isLoading = loadingPaths.has(path);
      if (!alreadyHaveChildren && !isLoading) {
        fetchChildren(path);
      }
    },

    collapse(path) {
      set((s) => {
        const expanded = new Set(s.expanded);
        expanded.delete(path);
        return { expanded };
      });
    },

    setOpenFlow(path) {
      set({ openFlow: path, selection: null });
      if (path === null) return;

      // Lazy fetch: load the subtree and links for the given flow path
      const { nodes } = get();
      const subtreeLoaded = [...nodes.values()].some((n) =>
        n.parent_path?.startsWith(path) || n.path.startsWith(path + "/"),
      );
      if (!subtreeLoaded) {
        fetchSubtree(path);
        fetchLinks(path);
      }
    },

    setSelection(path) {
      set({ selection: path });
    },

    // ----------------------------------------------------------------
    // Optimistic slot write
    // ----------------------------------------------------------------

    async writeSlot(path, slot, value) {
      const key = `${path}::${slot}`;
      const { nodes, pending } = get();
      const node = nodes.get(path);
      if (!node) return;

      const prevSlot = node.slots.find((s) => s.name === slot);
      const prevValue = prevSlot?.value ?? null;
      const prevGen = prevSlot?.generation ?? 0;

      // Reject if another write is already pending for this (path, slot)
      if (pending.has(key)) {
        console.warn(`[GraphStore] writeSlot: already pending for ${key}`);
        return;
      }

      // Optimistically update the cache
      set((s) => {
        const nodes = new Map(s.nodes);
        const n = nodes.get(path);
        if (!n) return {};
        const slots = n.slots.map((sl) =>
          sl.name === slot
            ? { ...sl, value, generation: sl.generation + 1 }
            : sl,
        );
        nodes.set(path, { ...n, slots });
        const pending = new Map(s.pending);
        pending.set(key, {
          expectedGen: prevGen + 1,
          value,
          prevValue,
          prevGen,
          startedAt: Date.now(),
        });
        return { nodes, pending };
      });

      // Set a 5 s timeout to revert if neither signal arrives
      const timeout = setTimeout(() => {
        const { pending } = get();
        if (!pending.has(key)) return; // already resolved
        console.warn(`[GraphStore] writeSlot timeout for ${key}, reverting`);
        revertSlot(path, slot, key, prevValue, prevGen);
      }, 5_000);

      // Fire the HTTP write
      try {
        const generation = await client.slots.writeSlot(path, slot, value);
        // HTTP response arrived — clear pending with matching gen
        resolvePendingWrite(path, slot, key, generation, prevValue, prevGen);
        clearTimeout(timeout);
      } catch (err) {
        // HTTP error — clear pending, revert cache
        clearTimeout(timeout);
        revertSlot(path, slot, key, prevValue, prevGen);
        console.error(`[GraphStore] writeSlot HTTP error for ${key}:`, err);
      }
    },

    // ----------------------------------------------------------------
    // applyEvent — idempotent cache mutations
    // ----------------------------------------------------------------

    applyEvent(event) {
      if (!initialFetchDone) {
        earlyQueue.push(event);
        return;
      }
      applyEventToCache(event, get, set, enqueueBatchedFetch);
      set({ lastSeq: event.seq });
    },

    // ----------------------------------------------------------------
    // reconcile — refetch all visible state after reconnect
    // ----------------------------------------------------------------

    reconcile() {
      const { expanded, openFlow } = get();
      const stalePaths = new Set<string>(expanded);
      if (openFlow) stalePaths.add(openFlow);

      for (const path of stalePaths) {
        if (expanded.has(path)) {
          fetchChildren(path);
        }
        if (openFlow && path === openFlow) {
          fetchSubtree(path);
          fetchLinks(path);
        }
      }
    },

    // ----------------------------------------------------------------
    // Internal merge helpers
    // ----------------------------------------------------------------

    _mergeNodes(snapshots) {
      set((s) => {
        const nodes = new Map(s.nodes);
        for (const n of snapshots) {
          // Re-apply any in-flight optimistic slot writes so a late batch-fetch
          // doesn't clobber a pending writeSlot.
          nodes.set(n.path, applyPendingSlots(n, s.pending));
        }
        return { nodes };
      });
    },

    _mergeLinks(newLinks) {
      set((s) => {
        const links = new Map(s.links);
        for (const l of newLinks) {
          links.set(l.id, l);
        }
        return { links };
      });
    },
  }));

  // ----------------------------------------------------------------
  // Private helpers (closures over `store` and `client`)
  // ----------------------------------------------------------------

  function fetchChildren(path: string) {
    set((s) => {
      const lp = new Set(s.loadingPaths);
      lp.add(path);
      return { loadingPaths: lp };
    });

    const filter = path === "/"
      ? "parent_path==/"
      : `parent_path==${path}`;

    client.nodes
      .getNodesPage({ filter, size: 1000 })
      .then((page) => {
        store.getState()._mergeNodes(page.data);
      })
      .catch(console.error)
      .finally(() => {
        set((s) => {
          const lp = new Set(s.loadingPaths);
          lp.delete(path);
          return { loadingPaths: lp };
        });
      });
  }

  function fetchSubtree(flowPath: string) {
    set((s) => {
      const lp = new Set(s.loadingPaths);
      lp.add(flowPath);
      return { loadingPaths: lp };
    });

    const prefix = flowPath.endsWith("/") ? flowPath : `${flowPath}/`;
    client.nodes
      .getNodesPage({ filter: `path=prefix=${prefix}`, size: 1000 })
      .then((page) => {
        // Also fetch the flow node itself
        return client.nodes.getNode(flowPath).then((root) => {
          store.getState()._mergeNodes([root, ...page.data]);
        });
      })
      .catch(console.error)
      .finally(() => {
        set((s) => {
          const lp = new Set(s.loadingPaths);
          lp.delete(flowPath);
          return { loadingPaths: lp };
        });
      });
  }

  async function fetchLinks(_flowPath: string) {
    try {
      const links = await client.links.list();
      store.getState()._mergeLinks(links);
    } catch (err) {
      console.error("[GraphStore] fetchLinks error:", err);
    }
  }

  function enqueueBatchedFetch(entry: PendingFetchEntry) {
    pendingFetches.set(entry.id, entry);
    if (fetchTimer !== null) return;
    fetchTimer = setTimeout(() => {
      fetchTimer = null;
      drainBatchedFetches();
    }, 25);
  }

  function drainBatchedFetches() {
    if (pendingFetches.size === 0) return;
    const BATCH_SIZE = 200;
    const entries = [...pendingFetches.values()];
    pendingFetches.clear();

    for (let i = 0; i < entries.length; i += BATCH_SIZE) {
      const batch = entries.slice(i, i + BATCH_SIZE);
      const ids = batch.map((e) => e.id).join(",");
      client.nodes
        .getNodesPage({ filter: `id=in=${ids}`, size: BATCH_SIZE })
        .then((page) => {
          store.getState()._mergeNodes(page.data);
          // Flip parent has_children flags
          const { nodes } = store.getState();
          const parents = new Set(
            page.data.map((n) => n.parent_path).filter(Boolean) as string[],
          );
          const updates: NodeSnapshot[] = [];
          for (const parentPath of parents) {
            const parent = nodes.get(parentPath);
            if (parent && !parent.has_children) {
              updates.push({ ...parent, has_children: true });
            }
          }
          if (updates.length > 0) {
            store.getState()._mergeNodes(updates);
          }
        })
        .catch(console.error);
    }
  }

  function resolvePendingWrite(
    _path: string,
    _slot: string,
    key: string,
    gen: number,
    prevValue: unknown,
    prevGen: number,
  ) {
    const { pending } = store.getState();
    const p = pending.get(key);
    if (!p) return; // already resolved

    if (gen > p.expectedGen) {
      // Conflict: server moved on → revert
      revertSlot(_path, _slot, key, prevValue, prevGen);
      return;
    }
    // Match: clear pending
    set((s) => {
      const pending = new Map(s.pending);
      pending.delete(key);
      return { pending };
    });
  }

  function revertSlot(
    path: string,
    slot: string,
    key: string,
    prevValue: unknown,
    prevGen: number,
  ) {
    set((s) => {
      const nodes = new Map(s.nodes);
      const n = nodes.get(path);
      if (n) {
        const slots = n.slots.map((sl) =>
          sl.name === slot
            ? { ...sl, value: prevValue, generation: prevGen }
            : sl,
        );
        nodes.set(path, { ...n, slots });
      }
      const pending = new Map(s.pending);
      pending.delete(key);
      const conflicts = new Map(s.conflicts);
      conflicts.set(key, prevValue); // signal to UI
      return { nodes, pending, conflicts };
    });
  }

  function applyEventToCache(
    event: GraphEvent,
    get: () => GraphStoreState,
    set: (fn: (s: GraphStoreState) => Partial<GraphStoreState>) => void,
    enqueueFetch: (entry: PendingFetchEntry) => void,
  ) {
    const { nodes, links, expanded, openFlow, selection } = get();

    switch (event.event) {
      case "node_created": {
        // Flip parent has_children flag if parent is cached
        const parentNode = [...nodes.values()].find(
          (n) => n.path === event.path.replace(/\/[^/]+$/, "") || false,
        );
        if (parentNode && !parentNode.has_children) {
          set((s) => {
            const updated = new Map(s.nodes);
            updated.set(parentNode.path, { ...parentNode, has_children: true });
            return { nodes: updated };
          });
        }
        // Fetch the full snapshot if the new node is visible
        const parentPath = event.path.includes("/")
          ? event.path.replace(/\/[^/]+$/, "") || "/"
          : "/";
        const visibleByExpanded = expanded.has(parentPath);
        const visibleByFlow = openFlow
          ? event.path.startsWith(openFlow + "/") || event.path === openFlow
          : false;
        if (visibleByExpanded || visibleByFlow) {
          enqueueFetch({
            id: event.id,
            path: event.path,
            parentPath,
          });
        }
        break;
      }

      case "node_removed": {
        const prefix = event.path + "/";
        set((s) => {
          const updated = new Map(s.nodes);
          // Remove the node and all descendants
          for (const [k] of updated) {
            if (k === event.path || k.startsWith(prefix)) {
              updated.delete(k);
            }
          }
          // Collapse if expanded
          const newExpanded = new Set(s.expanded);
          for (const e of newExpanded) {
            if (e === event.path || e.startsWith(prefix)) {
              newExpanded.delete(e);
            }
          }
          // Flip parent has_children if no more children remain
          const parentPath = event.path.includes("/")
            ? event.path.replace(/\/[^/]+$/, "") || "/"
            : "/";
          const parentNode = updated.get(parentPath);
          const parentStillHasChildren = parentNode
            ? [...updated.values()].some((n) => n.parent_path === parentPath)
            : false;
          const newNodes = parentNode
            ? new Map(updated).set(parentPath, {
                ...parentNode,
                has_children: parentStillHasChildren,
              })
            : updated;
          return {
            nodes: newNodes,
            expanded: newExpanded,
            openFlow:
              s.openFlow === event.path || s.openFlow?.startsWith(prefix)
                ? null
                : s.openFlow,
            selection:
              s.selection === event.path || s.selection?.startsWith(prefix)
                ? null
                : s.selection,
          };
        });
        break;
      }

      case "node_renamed": {
        const oldPrefix = event.old_path + "/";
        const newPrefix = event.new_path + "/";
        set((s) => {
          const updated = new Map<string, NodeSnapshot>();
          for (const [k, v] of s.nodes) {
            let newKey = k;
            let newNode = v;
            if (k === event.old_path) {
              newKey = event.new_path;
              newNode = { ...v, path: event.new_path };
            } else if (k.startsWith(oldPrefix)) {
              newKey = newPrefix + k.slice(oldPrefix.length);
              newNode = { ...v, path: newKey };
            }
            updated.set(newKey, newNode);
          }
          // Rewrite expanded set
          const newExpanded = new Set<string>();
          for (const e of s.expanded) {
            if (e === event.old_path) newExpanded.add(event.new_path);
            else if (e.startsWith(oldPrefix))
              newExpanded.add(newPrefix + e.slice(oldPrefix.length));
            else newExpanded.add(e);
          }
          // Rewrite link endpoints
          const newLinks = new Map<string, Link>();
          for (const [id, link] of s.links) {
            const rewrite = (p: string | null | undefined) => {
              if (!p) return p;
              if (p === event.old_path) return event.new_path;
              if (p.startsWith(oldPrefix)) return newPrefix + p.slice(oldPrefix.length);
              return p;
            };
            newLinks.set(id, {
              ...link,
              source: { ...link.source, path: rewrite(link.source.path) ?? undefined },
              target: { ...link.target, path: rewrite(link.target.path) ?? undefined },
            });
          }
          return {
            nodes: updated,
            links: newLinks,
            expanded: newExpanded,
            openFlow: s.openFlow === event.old_path
              ? event.new_path
              : s.openFlow?.startsWith(oldPrefix)
              ? newPrefix + s.openFlow.slice(oldPrefix.length)
              : s.openFlow,
            selection: s.selection === event.old_path
              ? event.new_path
              : s.selection?.startsWith(oldPrefix)
              ? newPrefix + s.selection.slice(oldPrefix.length)
              : s.selection,
          };
        });
        break;
      }

      case "slot_changed": {
        const { pending } = get();
        const key = `${event.path}::${event.slot}`;
        const p = pending.get(key);

        if (p) {
          // A pending write exists — use first-signal-wins rule
          resolvePendingWrite(event.path, event.slot, key, event.generation, p.prevValue, p.prevGen);
        }

        set((s) => {
          const n = s.nodes.get(event.path);
          if (!n) return {};
          const existingSlot = n.slots.find((sl) => sl.name === event.slot);
          if (existingSlot && event.generation <= existingSlot.generation) {
            return {}; // stale, ignore
          }
          const slots = n.slots.some((sl) => sl.name === event.slot)
            ? n.slots.map((sl) =>
                sl.name === event.slot
                  ? { ...sl, value: event.value, generation: event.generation }
                  : sl,
              )
            : [
                ...n.slots,
                { name: event.slot, value: event.value, generation: event.generation },
              ];
          const updated = new Map(s.nodes);
          updated.set(event.path, { ...n, slots });
          return { nodes: updated };
        });
        break;
      }

      case "lifecycle_transition": {
        set((s) => {
          const n = s.nodes.get(event.path);
          if (!n) return {};
          const updated = new Map(s.nodes);
          updated.set(event.path, { ...n, lifecycle: event.to });
          return { nodes: updated };
        });
        break;
      }

      case "link_added": {
        set((s) => {
          const updated = new Map(s.links);
          updated.set(event.id, {
            id: event.id,
            source: { node_id: event.source.node, slot: event.source.slot },
            target: { node_id: event.target.node, slot: event.target.slot },
          });
          return { links: updated };
        });
        break;
      }

      case "link_removed":
      case "link_broken": {
        set((s) => {
          const updated = new Map(s.links);
          updated.delete(event.id);
          return { links: updated };
        });
        break;
      }

      default:
        break;
    }
  }

  /** Re-apply any pending optimistic slot writes on top of an incoming snapshot. */
  function applyPendingSlots(snapshot: NodeSnapshot, pending: Map<string, PendingWrite>): NodeSnapshot {
    const prefix = snapshot.path + "::"; 
    const entries = [...pending.entries()].filter(([k]) => k.startsWith(prefix));
    if (entries.length === 0) return snapshot;
    const slots = [...snapshot.slots];
    for (const [key, pw] of entries) {
      const slotName = key.slice(prefix.length);
      const idx = slots.findIndex((s) => s.name === slotName);
      const existing = slots[idx];
      if (existing !== undefined) {
        if (existing.generation < pw.expectedGen) {
          slots[idx] = { name: slotName, value: pw.value, generation: pw.expectedGen };
        }
      } else {
        slots.push({ name: slotName, value: pw.value, generation: pw.expectedGen });
      }
    }
    return { ...snapshot, slots };
  }

  // Keep set reference for helpers
  function set(fn: (s: GraphStoreState) => Partial<GraphStoreState>) {
    store.setState((s) => ({ ...s, ...fn(s) }));
  }

  // ----------------------------------------------------------------
  // SSE startup: subscribe-before-fetch protocol
  // ----------------------------------------------------------------

  type EventSub = AsyncIterable<GraphEvent> & { close(): void; readonly lastSeq: number };
  let sub: EventSub | null = null;

  function startSSE() {
    const { lastSeq } = store.getState();
    sub = client.events.subscribe({
      ...(lastSeq > 0 && { sinceSeq: lastSeq }),
      onOpen(seq) {
        store.setState({ lastSeq: seq });
      },
      onCursorTooOld() {
        // Ring expired — full reconcile
        sub?.close();
        sub = null;
        initialFetchDone = false;
        void start();
      },
    });

    const currentSub = sub;
    void (async () => {
      for await (const event of currentSub) {
        store.getState().applyEvent(event);
      }
    })();
  }

  async function start() {
    earlyQueue.length = 0;
    startSSE();

    // Fetch initial visible data
    const { expanded, openFlow } = store.getState();
    const fetches: Promise<unknown>[] = [];

    for (const path of expanded) {
      const filter = path === "/" ? "parent_path==/" : `parent_path==${path}`;
      fetches.push(
        client.nodes
          .getNodesPage({ filter, size: 1000 })
          .then((page) => store.getState()._mergeNodes(page.data)),
      );
    }

    if (openFlow) {
      const prefix = openFlow.endsWith("/") ? openFlow : `${openFlow}/`;
      fetches.push(
        client.nodes
          .getNodesPage({ filter: `path=prefix=${prefix}`, size: 1000 })
          .then((page) => store.getState()._mergeNodes(page.data)),
      );
      fetches.push(
        client.links.list().then((ls) => store.getState()._mergeLinks(ls)),
      );
    }

    await Promise.all(fetches).catch(console.error);

    // Drain early queue
    initialFetchDone = true;
    for (const event of earlyQueue.splice(0)) {
      applyEventToCache(
        event,
        store.getState,
        (fn) => store.setState((s) => ({ ...s, ...fn(s) })),
        enqueueBatchedFetch,
      );
      store.setState({ lastSeq: event.seq });
    }
  }

  // Boot
  void start();

  /** Tear down the SSE connection (call on component unmount / logout). */
  function destroy() {
    sub?.close();
    sub = null;
  }

  return Object.assign(store, { destroy });
}

export type GraphStore = ReturnType<typeof createGraphStore>;
