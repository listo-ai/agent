Task: Stage 3a-2 — NodeCtx + BehaviorRegistry + acme.compute.count.
First real behaviour kind end-to-end through the new dispatch seam.

Read first (in order, before touching code):

  docs/design/NEW-SESSION.md
       — especially the READ-THIS section with Rule A + Rule B.
         This stage is where Rule A lands at per-instance granularity.

  docs/design/EVERYTHING-AS-NODE.md
       — § "The agent itself is a node too — no parallel state"
         is the template. Behaviours follow the same rule at the
         instance level.

  docs/design/NODE-AUTHORING.md
       — § "Behaviours are stateless — state lives in slots" is
         the rule for this stage. No per-instance fields on behaviour
         structs. Trait takes &self, not &mut self.

  docs/sessions/NODE-SCOPE.md
       — the acme.compute.count example shows the exact corrected
         shape (unit struct, no self.value, slot-backed state).

  docs/sessions/STEPS.md § Stage 3a-2
       — full deliverable list and the slot-source regression test.

  docs/design/TESTS.md
       — how tests are structured here. Specifically the
         "write-through / fail-closed" pattern — use it for the
         slot-source regression test.

Survey before coding:

  crates/extensions-sdk/src/         (current trait stubs; 3a-1 landed)
  crates/extensions-sdk-macros/src/  (derive macro; may not need changes)
  crates/engine/src/                 (live-wire executor + queue;
                                       BehaviorRegistry slots in beside it)
  crates/graph/src/store.rs          (slot read/write — NodeCtx wraps this)
  crates/spi/src/containment.rs      (Slot, SlotSchema — NodeCtx consumes)
  crates/spi/src/capabilities.rs     (requires! macro input types)

Scope — what ships in 3a-2:

1. NodeCtx real surface in extensions-sdk (under native feature for now;
   wasm/process feature impls stub-only for 3a-2):
     - emit(port: &str, msg: Msg) -> Result<(), NodeError>
     - read_slot(path: &NodePath, slot: &str) -> Result<Value, NodeError>
     - read_status(slot: &str) -> Result<Value, NodeError>        (this node)
     - read_config(slot: &str) -> Result<Value, NodeError>        (this node)
     - update_status(slot: &str, value: Value) -> Result<(), NodeError>
     - schedule — stubbed; real impl in 3a-3 for trigger
     - resolve_settings(msg: &Msg) -> Result<ResolvedSettings<T>, NodeError>
     - logger — route via observability::prelude once Stage 2d's
       wiring lands; for now tracing:: with canonical fields is fine
       but flag the call sites so 2d can find them.

   NodeCtx binds to a specific NodeId at dispatch time; graph access
   goes through a trait object (GraphAccess or similar) so tests can
   mock it cheaply.

2. NodeBehavior trait update in extensions-sdk:
     - &self methods, not &mut self. This is the compiler-enforced rule.
     - on_init(ctx, cfg), on_message(ctx, port, msg),
       on_config_change(ctx, cfg), on_shutdown(ctx).
     - No default panicking impl of on_message — required.

3. Settings / ResolvedSettings<T> in extensions-sdk:
     - Resolution order: msg > config > schema default.
     - Reads msg_overrides map from the manifest; if msg has the field
       listed there (under metadata.<msg_field>), it wins.
     - Validates the merged result against the settings schema before
       returning.

4. BehaviorRegistry in crates/engine:
     - HashMap<KindId, BehaviorEntry> where BehaviorEntry holds the
       dispatch fn and any kind-level singleton resources.
     - Registered at boot from the kinds in spi KindRegistry that
       declared behavior = "custom" at derive time. Native-feature
       kinds self-register via linkme or inventory — justify the
       choice in the design sketch.
     - Dispatcher: subscribes to GraphEvent::SlotChanged, filters for
       trigger:true input slots, looks up kind, calls on_message with
       NodeCtx bound to the target node. Lives beside the live-wire
       executor; both consume the same event stream.

5. requires! macro in extensions-sdk:
     - Emits const REQUIRES: &[Requirement] = &[...];
     - Consumes spi::capabilities::{CapabilityId, SemverRange}.
     - First user is acme.compute.count, which requires spi.msg@^1.

6. New crate crates/domain-compute/:
     - Houses acme.compute.count.
     - Unit struct pub struct Count; with #[derive(NodeKind)]
       (behavior = "custom"). NO per-instance fields.
     - CountConfig with serde + SettingsSchema derive.
     - impl NodeBehavior for Count with on_init + on_message matching
       the NODE-SCOPE corrected example — read current count from the
       status slot via ctx.read_status("count"), not from self.

7. Manifest: crates/domain-compute/manifests/count.yaml
     - Matches the NODE-SCOPE manifest section: two inputs, one output,
       status slot count, settings schema, trigger_policy on_any,
       msg_overrides for step/reset/initial.

8. Tests:
     - Unit: apply_step() arithmetic — step, clamp, wrap, negative step.
     - Unit: resolve_settings resolution order (msg > config > default).
     - Integration (behaviour dispatch): create a count node in a test
       graph, wire an upstream "number" source, write a value, assert
       count slot and out port both reflect the increment.
     - SLOT-SOURCE REGRESSION TEST (required, per STEPS 3a-2):
         1. Create count node with initial=10.
         2. Write count slot directly via GraphStore::write_slot to 42.
         3. Fire an "in" message.
         4. Assert emitted out value is 43 (42+1), NOT 11 (10+1).
         This catches any attempt to cache slot state in a struct field.
     - Reset test: both port="reset" AND msg.reset=true reset to initial.
     - Multi-input msg_overrides: msg.step=5 with step=1 config uses 5.

9. Deferrals in 3a-2 (call out in the design sketch so they don't silently slip):
     - Timers (NodeCtx::schedule real impl) — 3a-3 with trigger.
     - wasm/process adapter impls of NodeCtx — stub-only for 3a-2;
       real in 3b/3c.
     - acme.logic.trigger — 3a-3.

Constraints:
  - Non-negotiables #1-#7 from NEW-SESSION. Especially #1.
  - File cap 400, function cap 50 per CODE-LAYOUT.
  - TDD per docs/design/TESTS.md — tests arrive in the same PR, fail
    if the implementation is reverted.
  - No println!/eprintln! in library code; tracing is allowed for now
    but flag call sites so Stage 2d's observability wiring knows where
    they are.
  - fmt --check, clippy --workspace --all-targets -- -D warnings, and
    cargo test --workspace must stay green.

Before coding: post a 30-second design sketch for my eyeballs — same
pattern as 3a-1. I want to see:
  - NodeCtx exact method signatures and which trait it sits behind
    for mockability.
  - How BehaviorRegistry discovers kinds (linkme vs inventory vs
    explicit registration in domain-compute's lib.rs).
  - Dispatch path from SlotChanged → NodeCtx → on_message, written
    as ~5 bullets.
  - Any open trade-off worth a decision before typing.

Then I'll green-light and you code.