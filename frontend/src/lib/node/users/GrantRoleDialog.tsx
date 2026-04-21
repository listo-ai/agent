/**
 * GrantRoleDialog — pure presentational dialog for role-grant requests.
 *
 * Zero business logic. Drive it by pairing:
 *   - `useGrantRole()` for the mutation state
 *   - `open` / `onOpenChange` to control visibility
 *
 * @example
 * ```tsx
 * const grant = useGrantRole();
 * const [open, setOpen] = useState(false);
 * const [targetId, setTargetId] = useState<string | null>(null);
 *
 * <GrantRoleDialog
 *   open={open}
 *   onOpenChange={setOpen}
 *   userId={targetId ?? ""}
 *   grantState={grant}
 * />
 * ```
 */
import { useState, useId } from "react";
import { Loader2, CheckCircle2, AlertCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { GrantRoleState } from "./types";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface GrantRoleDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The `sys.auth.user` node id receiving the grant. */
  userId: string;
  grantState: GrantRoleState;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function GrantRoleDialog({
  open,
  onOpenChange,
  userId,
  grantState,
}: GrantRoleDialogProps) {
  const roleId = useId();
  const bulkId = useId();
  const [role, setRole] = useState("");
  const [bulkActionId, setBulkActionId] = useState("");

  const isPending = grantState.status === "pending";
  const isOk = grantState.status === "ok";

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!role.trim() || !bulkActionId.trim()) return;
    void grantState.grant(userId, role.trim(), bulkActionId.trim());
  }

  function handleClose(next: boolean) {
    if (!next) {
      // Reset local + external state on close.
      setRole("");
      setBulkActionId("");
      grantState.reset();
    }
    onOpenChange(next);
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle>Grant Role</DialogTitle>
        </DialogHeader>

        {isOk ? (
          <div className="flex flex-col items-center gap-3 py-6 text-center">
            <CheckCircle2 className="h-10 w-10 text-green-500" aria-hidden />
            <p className="text-sm font-medium">Role granted — request accepted.</p>
            <Button variant="outline" onClick={() => handleClose(false)}>
              Close
            </Button>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4 pt-2">
            <div className="space-y-1.5">
              <Label htmlFor={roleId}>Role</Label>
              <Input
                id={roleId}
                placeholder="e.g. admin, viewer"
                value={role}
                onChange={(e) => setRole(e.target.value)}
                disabled={isPending}
                required
              />
            </div>

            <div className="space-y-1.5">
              <Label htmlFor={bulkId}>Bulk Action ID</Label>
              <Input
                id={bulkId}
                placeholder="Correlation ID for this batch"
                value={bulkActionId}
                onChange={(e) => setBulkActionId(e.target.value)}
                disabled={isPending}
                required
              />
              <p className="text-[11px] text-muted-foreground">
                Ties this grant to an audit session.
              </p>
            </div>

            {grantState.status === "error" && (
              <div className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                {grantState.errorDetail}
              </div>
            )}

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => handleClose(false)}
                disabled={isPending}
              >
                Cancel
              </Button>
              <Button type="submit" disabled={isPending || !role.trim() || !bulkActionId.trim()}>
                {isPending && <Loader2 className="mr-2 h-4 w-4 animate-spin" aria-hidden />}
                Grant
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
