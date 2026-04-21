import { createPortal } from "react-dom";
import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { Kind } from "@listo/agent-client";
import { cn } from "@/lib/utils";
import { useAgent } from "@/hooks/useAgent";
import { useCreateNode } from "@/lib/node";

interface AddChildNodeDialogProps {
  /** Graph path of the parent node (e.g. "/my-flow/device-a"). */
  parentPath: string;
  onClose: () => void;
  /** Called after the child node is successfully created. */
  onCreated: (createdPath: string) => void;
}

/**
 * Modal dialog that lets the user pick a kind and enter a name to create a
 * child node under `parentPath`.
 *
 * Queries GET /api/v1/kinds?placeable_under=<parentPath> so only valid kinds
 * are shown (respecting containment rules).
 */
export function AddChildNodeDialog({
  parentPath,
  onClose,
  onCreated,
}: AddChildNodeDialogProps) {
  const agentQuery = useAgent();
  const createNode = useCreateNode();
  const [selectedKind, setSelectedKind] = useState<Kind | null>(null);
  const [name, setName] = useState("");
  const nameRef = useRef<HTMLInputElement>(null);

  const kindsQuery = useQuery({
    queryKey: ["kinds-placeable-under", parentPath],
    queryFn: () => agentQuery.data!.kinds.listPlaceableUnder(parentPath),
    enabled: agentQuery.data !== undefined,
    staleTime: 30_000,
  });

  const kinds = kindsQuery.data ?? [];

  // Auto-select the first kind once loaded.
  useEffect(() => {
    if (kinds.length > 0 && !selectedKind) {
      setSelectedKind(kinds[0] ?? null);
    }
  }, [kinds, selectedKind]);

  // Auto-focus the name field once it appears.
  useEffect(() => {
    if (!kindsQuery.isPending) {
      nameRef.current?.focus();
    }
  }, [kindsQuery.isPending]);

  // Close on Escape.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedKind || !name.trim()) return;
    try {
      const path = await createNode.mutateAsync({
        parent: parentPath,
        kind: selectedKind.id,
        name: name.trim(),
      });
      onCreated(path);
    } catch {
      // error surfaced via createNode.error below
    }
  };

  return createPortal(
    // Backdrop
    <div
      className="fixed inset-0 z-[9998] flex items-center justify-center bg-black/40 backdrop-blur-sm"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        className="z-[9999] w-full max-w-sm overflow-hidden rounded-2xl border border-border bg-card shadow-2xl"
        onMouseDown={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="border-b border-border px-5 py-4">
          <h2 className="text-sm font-semibold">Add child node</h2>
          <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
            {parentPath}
          </p>
        </div>

        <form onSubmit={(e) => void handleSubmit(e)} className="p-5">
          {/* Kind picker */}
          <label className="mb-1 block text-xs font-medium text-muted-foreground">
            Kind
          </label>
          {kindsQuery.isPending ? (
            <div className="flex h-9 items-center rounded-lg border border-border bg-background px-3 text-sm text-muted-foreground">
              Loading kinds…
            </div>
          ) : kinds.length === 0 ? (
            <div className="flex h-9 items-center rounded-lg border border-dashed border-border bg-background px-3 text-sm text-muted-foreground">
              No placeable kinds found
            </div>
          ) : (
            <select
              className="block w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              value={selectedKind?.id ?? ""}
              onChange={(e) => {
                const k = kinds.find((k) => k.id === e.target.value);
                setSelectedKind(k ?? null);
              }}
            >
              {kinds.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.id}
                </option>
              ))}
            </select>
          )}

          {/* Name input */}
          <label className="mb-1 mt-4 block text-xs font-medium text-muted-foreground">
            Name
          </label>
          <input
            ref={nameRef}
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. device-a"
            className="block w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-2 focus:ring-ring"
          />
          <p className="mt-1 text-[11px] text-muted-foreground">
            Path: <code className="font-mono">{parentPath}/{name || "…"}</code>
          </p>

          {/* Error */}
          {createNode.error && (
            <p className="mt-3 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-sm text-destructive">
              {createNode.error instanceof Error ? createNode.error.message : String(createNode.error)}
            </p>
          )}

          {/* Actions */}
          <div className="mt-5 flex justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg border border-border bg-background px-4 py-2 text-sm hover:bg-accent"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!selectedKind || !name.trim() || createNode.isPending || kinds.length === 0}
              className={cn(
                "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground",
                "disabled:pointer-events-none disabled:opacity-40",
                "hover:opacity-90",
              )}
            >
              {createNode.isPending ? "Creating…" : "Create"}
            </button>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  );
}
