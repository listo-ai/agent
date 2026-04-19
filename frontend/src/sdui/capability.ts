/**
 * IR version handshake — the client refuses to project trees whose
 * `ir_version` exceeds what it knows how to render. Mirrors the
 * backend constant (`ui_ir::IR_VERSION`).
 *
 * Bumping: adding a component variant is a minor bump (back-compat,
 * this constant stays); removing or re-shaping is a major bump and
 * this constant follows. Keep aligned with
 * [crates/ui-ir/src/lib.rs](../../../crates/ui-ir/src/lib.rs).
 */
import type { UiComponentTree } from "@sys/agent-client";

export const SUPPORTED_IR_VERSION = 1;

export type CapabilityMismatch = {
  kind: "capability-mismatch";
  supported: number;
  received: number;
};

/**
 * Returns a mismatch descriptor if the tree's `ir_version` is higher
 * than what we can render; otherwise returns `null`. Lower versions
 * (e.g. the server clamped emission to an older IR) are accepted —
 * that's the back-compat path.
 */
export function checkIrVersion(tree: UiComponentTree): CapabilityMismatch | null {
  if (tree.ir_version > SUPPORTED_IR_VERSION) {
    return {
      kind: "capability-mismatch",
      supported: SUPPORTED_IR_VERSION,
      received: tree.ir_version,
    };
  }
  return null;
}
