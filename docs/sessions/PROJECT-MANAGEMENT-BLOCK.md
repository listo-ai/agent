# Session — Project Management Block & Reusable SDK Libraries

Scoping a Project Management example block as the forcing function for building common, reusable SDK libraries that future-proof the block framework. The block itself is secondary — the shared libraries it proves are the deliverable.

## Problem statement

Today, each block that wants a frontend UI must wire its own Module Federation remote, import the TS client directly, and hand-roll component code. There is no shared block-UI SDK, no common Rust domain-logic library for blocks, and no standard patterns for connecting SDUI views to block-authored domain types. The existing `com.acme.hello` and `com.acme.wasm-demo` blocks prove the Wasm SDK path but exercise none of the SDUI, client, or frontend-component reuse story.

A Project Management block (boards, tasks, states, assignments) is CRUD-heavy, has multiple views (board, list, detail, form), and needs live updates — exactly the long-tail shape SDUI was designed for. Building it will expose every gap in the reusable-library story.

## Goals

1. **Validate and harden three reusable libraries** that every future block can depend on.
2. Build `com.acme.project` as a non-trivial reference block exercising all three.
3. Prove that a block author ships **zero custom frontend code** for CRUD screens via SDUI, and minimal MF code only for specialist views (e.g. a Kanban board).
4. Demonstrate the full block lifecycle: kind manifests → SDUI views → action handlers → live subscriptions → optional `custom` MF widget.

## Non-goals

- Production-grade project management (no Gantt, no resource levelling, no calendar sync).
- Replacing any existing domain crate — this block lives entirely under `blocks/`.
- New IR components — the existing ~25 component vocabulary plus `custom` must be sufficient; if it isn't, that's a finding, not a deliverable.
- Process-block or Wasm execution — the block registers kind manifests + SDUI views + action handlers. Backend behaviour is native Rust linked in-tree (per [BLOCKS.md](../design/BLOCKS.md) Stage 3a pattern) until the process-block supervisor lands.

---

## The three reusable libraries

### 1. `@sys/block-ui` — Frontend Block UI SDK (TypeScript / React)

**Location:** `clients/block-ui/` (new pnpm workspace package).

**Purpose:** Common React primitives and hooks for any block that ships a Module Federation UI bundle. Sits between the raw `@sys/agent-client` TS client and the block's own components.

**What it provides:**

| Export | Description |
|--------|-------------|
| `useAgentClient()` | Hook returning the pre-configured `AgentClient` from Studio's context — blocks never construct their own. |
| `useNode(path)` | Hook returning a node snapshot + live subscription. Wraps `client.nodes.get()` + `client.events.subscribe()` with React Query cache integration. |
| `useSlot(path, slotName)` | Hook returning a slot value + live updates. Wrapper over `useNode` scoped to one slot. |
| `useNodes(query)` | Hook for querying/listing nodes with pagination, sort, filter. Wraps `client.nodes.list()`. |
| `useAction(handler)` | Hook returning a `fire(args)` function that calls `POST /api/v1/ui/action` and handles the response union (toast, navigate, patch, form_errors). |
| `useSubscription(subjects)` | Hook that opens an SSE subscription scoped to given NATS subjects and invalidates matching React Query entries. |
| `BlockShell` | Layout wrapper honouring the Studio's panel contract (sidebar slot, property-panel slot, full-page slot). Handles loading/error states. |
| `NodeLink` | Component rendering a clickable node reference (path → Studio navigation). |
| `SlotBadge` | Renders a `<Badge>` driven by a slot value + intent mapping. |
| `registerBlockComponent(id, component)` | Registers a React component under a `renderer_id` for the SDUI `custom` variant — thin wrapper around the existing client-side registry in `frontend/src/sdui/`. |

**Design rules:**
- Zero domain knowledge — no "project", "task", "board" types. Only graph primitives.
- Depends on `@sys/agent-client` and React 19. Does **not** depend on `@sys/studio` internals.
- Exported as ESM. Consumed by block MF bundles via `shared` singleton (same as React/zustand).
- Size budget: **under 1500 lines** excluding tests.

**Why it matters for the framework:**
Every future block that ships a MF bundle (UC1 floor-plan, UC2 AI review UI, UC3 scope-board) needs these hooks. Without a shared package, each block re-invents client wiring, subscription management, and Studio integration.

---

### 2. `block-client` — Rust Block Client Helpers (Rust library crate)

**Location:** `crates/block-client/` (new workspace member).

**Purpose:** Thin convenience layer over `agent-client` (the Rust client in `clients/rs/`) for Rust code running inside blocks — action handlers, process-block binaries, integration tests, CLI scripts.

**What it provides:**

| Export | Description |
|--------|-------------|
| `BlockContext` | Struct holding the agent base URL + block ID + auth token. Built from env vars the block supervisor injects. |
| `BlockClient` | Wraps `AgentClient` with block-scoped defaults (e.g. queries auto-scoped to the block's namespace). |
| `ActionResult` | Ergonomic enum matching the SDUI action response union — `Toast`, `Navigate`, `FullRender`, `FormErrors`, `Patch`, `Download`, `None`. Serializes to the JSON the transport expects. |
| `view!{}` macro (stretch) | Proc-macro or builder DSL for authoring `ComponentTree` literals in Rust — compile-time validated against `ui-ir` types. Avoids hand-writing JSON for default views in kind manifests. |
| `test_harness` (dev-only) | Test utilities: spin up an in-process agent, register kinds, seed nodes, assert SDUI resolve output. |

**Design rules:**
- Depends on `agent-client`, `ui-ir`, `spi`. Does **not** depend on `graph`, `engine`, or any data crate.
- Library crate — `thiserror` for errors, no `anyhow`.
- Feature-gated `test_harness` behind `cfg(test)` / `dev-dependencies`.

**Why it matters for the framework:**
Action handlers are the backend half of every SDUI screen. Today each domain crate hand-rolls the response shape. A shared `ActionResult` type + `BlockContext` is the minimum viable reuse surface; the `view!{}` macro makes declaring SDUI views in kind manifests ergonomic and type-safe.

---

### 3. `block-domain` — Common Block Domain Types (Rust library crate)

**Location:** `crates/block-domain/` (new workspace member).

**Purpose:** Shared domain patterns that recur across blocks but don't belong in `spi` (which is framework contracts, not block-author convenience).

**What it provides:**

| Export | Description |
|--------|-------------|
| `StateMachine<S>` | Generic state machine with legal-transition table, `thiserror` violation errors, and audit-event emission. Blocks define their own state enums; the machine enforces transitions. |
| `Prioritised<T>` | Wrapper adding `priority: u16` + ordering. Used by tasks, alarms, work orders — any block with ranked items. |
| `AssignmentSet` | Set of `(NodeId, role: String)` pairs. Reusable for task assignment, device ownership, scope membership. |
| `TagFilter` | Filter expression over tags (the `domain-tags` + `query` crate intersection). Blocks use tags for categorisation; this gives them a typed filter without depending on `query` directly. |
| `Auditable` trait | Marker trait + blanket impl that emits an audit event on state transitions. Wires into the `audit` crate's `AuditSink`. |
| `SlotHelpers` | Extension trait on `Msg` / `serde_json::Value` for safe nested slot reads — `msg.dot_path("payload.state")` returning `Option<&Value>`. Avoids every block reimplementing the same JSON walker. |

**Design rules:**
- Pure domain — no HTTP, no SQL, no `tokio` (unless behind an `async` feature gate for the audit sink).
- Depends on `spi`, `audit` (trait only). Does **not** depend on `graph` or any data/transport crate.
- Every type derives `Serialize`, `Deserialize`, `JsonSchema`.
- Library crate — `thiserror` for errors.

**Why it matters for the framework:**
State machines, priority ordering, assignment sets, and tag filters are patterns that will appear in every non-trivial block (alarms, work orders, tickets, schedules, commissioning workflows). Extracting them now, forced by a real example, prevents N parallel reimplementations.

---

## The example block: `com.acme.project`

### Domain model (graph nodes)

| Kind ID | Parent | Slots | Description |
|---------|--------|-------|-------------|
| `com.acme.project.board` | `sys.core.folder` | `name`, `description`, `settings` (JSON Schema — columns, WIP limits) | A project board. Container for lists. |
| `com.acme.project.list` | `com.acme.project.board` | `name`, `position: u16`, `color` | A column / status bucket (e.g. "To Do", "In Progress", "Done"). |
| `com.acme.project.task` | `com.acme.project.list` | `title`, `description` (markdown), `state` (via `StateMachine`), `priority` (via `Prioritised`), `assignees` (via `AssignmentSet`), `tags`, `due_date` | A task card. |
| `com.acme.project.comment` | `com.acme.project.task` | `body` (markdown), `author`, `created_at` | A comment on a task. |

### Kind manifests with SDUI views

Each kind declares `views` in its YAML manifest per [SDUI.md](../design/SDUI.md) S5:

**`com.acme.project.board` — overview view:**
```yaml
views:
  - id: overview
    title: "Board Overview"
    priority: 1
    template:
      type: page
      ir_version: 1
      title: "{{$target.name}}"
      children:
        - type: row
          children:
            - type: heading
              content: "{{$target.name}}"
              level: 2
            - type: badge
              label: "{{$target/settings.wip_limit}} WIP"
              intent: info
        - type: table
          source:
            query: "parent_path=prefix={{$target.path}} AND kind==com.acme.project.task"
            subscribe: true
          columns:
            - { title: "Task", field: "slots.title.value" }
            - { title: "State", field: "slots.state.value" }
            - { title: "Priority", field: "slots.priority.value", sortable: true }
            - { title: "Assignee", field: "slots.assignees.value" }
            - { title: "Due", field: "slots.due_date.value", sortable: true }
          row_action: { handler: navigate, args: { target_ref: "$row.id" } }
          page_size: 50
```

**`com.acme.project.task` — detail view:**
```yaml
views:
  - id: detail
    title: "Task Detail"
    priority: 1
    template:
      type: page
      ir_version: 1
      title: "{{$target.slots.title.value}}"
      children:
        - type: row
          children:
            - type: heading
              content: "{{$target.slots.title.value}}"
              level: 2
            - type: badge
              label: "{{$target.slots.state.value}}"
              intent: "{{$target.slots.state.value}}"
        - type: form
          schema_ref: "$target.settings_schema"
          bindings: "$target.settings"
          submit:
            handler: com.acme.project.update_task
            args: { target: "$target.id" }
        - type: markdown
          content: "{{$target.slots.description.value}}"
        - type: heading
          content: "Comments"
          level: 3
        - type: table
          source:
            query: "parent_path=={{$target.path}} AND kind==com.acme.project.comment"
            subscribe: true
          columns:
            - { title: "Author", field: "slots.author.value" }
            - { title: "Comment", field: "slots.body.value" }
            - { title: "Date", field: "slots.created_at.value", sortable: true }
          page_size: 20
        - type: form
          schema_ref: "inline"
          fields:
            - { name: "body", type: "string", format: "markdown", label: "Add comment" }
          submit:
            handler: com.acme.project.add_comment
            args: { target: "$target.id" }
```

These views render through SDUI with **zero frontend code** — the existing renderer handles page, row, heading, badge, table, form, markdown.

### Custom MF widget: Kanban board

The one specialist view — a drag-and-drop Kanban board — ships as a `custom` IR variant backed by a small MF bundle:

```yaml
views:
  - id: kanban
    title: "Kanban Board"
    priority: 2
    template:
      type: page
      ir_version: 1
      title: "{{$target.name}} — Kanban"
      children:
        - type: custom
          renderer_id: com.acme.project.kanban
          props:
            board_path: "{{$target.path}}"
          subscribe:
            - "node.{{$target.id}}.slot.*"
```

**`blocks/com.acme.project/ui/`** ships a small MF bundle (~500 lines) containing:
- `KanbanBoard.tsx` — renders lists as columns, tasks as draggable cards.
- Uses `@sys/block-ui` hooks: `useNodes()` for lists/tasks, `useAction()` for move-task, `useSubscription()` for live updates.
- Registered via `registerBlockComponent("com.acme.project.kanban", KanbanBoard)`.

This proves the `@sys/block-ui` SDK end-to-end.

### Action handlers

Registered in the block's backend (native Rust, linked in-tree):

| Handler | Action | Notes |
|---------|--------|-------|
| `com.acme.project.create_task` | Creates a `com.acme.project.task` node under the target list | Validates WIP limits via `StateMachine` |
| `com.acme.project.update_task` | Writes slots on a task | Uses `ActionResult::Toast` on success |
| `com.acme.project.move_task` | Moves task between lists (graph move + state transition) | Uses `StateMachine` for legal transitions |
| `com.acme.project.add_comment` | Creates a `com.acme.project.comment` child | Returns `ActionResult::Patch` to append the new row |
| `com.acme.project.delete_task` | Cascading delete of task + comments | Returns `ActionResult::Navigate` back to board |

Each handler uses `block-client::ActionResult` and `block-domain::StateMachine`.

---

## Delivery stages

### P0 — Shared libraries (framework value)

| # | Deliverable | Crate/Package | Est. LOC | Depends on |
|---|-------------|---------------|----------|------------|
| P0.1 | `@sys/block-ui` — hooks + shell + registration | `clients/block-ui/` | ~1200 | `@sys/agent-client`, React 19 |
| P0.2 | `block-client` — `BlockContext`, `ActionResult`, test harness | `crates/block-client/` | ~600 | `agent-client`, `ui-ir`, `spi` |
| P0.3 | `block-domain` — `StateMachine`, `Prioritised`, `AssignmentSet`, `SlotHelpers` | `crates/block-domain/` | ~800 | `spi`, `audit` |

**Acceptance:** Each library compiles, has unit tests, and is documented with `///` doc comments satisfying `cargo doc`. `@sys/block-ui` passes `tsc --noEmit`. No domain-specific types in any of the three.

### P1 — Kind manifests + SDUI views (block skeleton)

| # | Deliverable | Location |
|---|-------------|----------|
| P1.1 | `block.yaml` for `com.acme.project` | `blocks/com.acme.project/block.yaml` |
| P1.2 | Kind manifests: `board`, `list`, `task`, `comment` | `blocks/com.acme.project/kinds/*.yaml` |
| P1.3 | SDUI views declared in manifests | Inline in kind YAML `views:` blocks |
| P1.4 | Kind registration in `domain-blocks` scan | Uses existing `BlockRegistry::scan` path |

**Acceptance:** `agent` starts, scans `com.acme.project`, registers 4 kinds, `GET /api/v1/blocks` lists it as `Enabled`. `GET /api/v1/ui/render?target=<board-id>` returns a valid `ComponentTree`.

### P2 — Action handlers (backend logic)

| # | Deliverable | Location |
|---|-------------|----------|
| P2.1 | Native Rust handler module | `blocks/com.acme.project/src/handlers.rs` (or inline crate) |
| P2.2 | 5 action handlers using `block-client::ActionResult` + `block-domain::StateMachine` | Same |
| P2.3 | Integration tests via `block-client::test_harness` | `blocks/com.acme.project/tests/` |

**Acceptance:** `POST /api/v1/ui/action { handler: "com.acme.project.create_task", … }` returns a valid response. State transitions enforce legal moves. Illegal transition returns `ActionResult::FormErrors`.

### P3 — Kanban MF widget (frontend SDK proof)

| # | Deliverable | Location |
|---|-------------|----------|
| P3.1 | `KanbanBoard.tsx` using `@sys/block-ui` hooks | `blocks/com.acme.project/ui/src/` |
| P3.2 | Rsbuild MF config + build | `blocks/com.acme.project/ui/rsbuild.config.ts` |
| P3.3 | Registration via `registerBlockComponent` | Entry module |

**Acceptance:** Navigating to a board in Studio, selecting the "Kanban Board" view tab, renders the drag-and-drop board. Moving a card fires `com.acme.project.move_task`. Live updates via subscription show changes from other sessions.

---

## File layout

```
blocks/
  com.acme.project/
    block.yaml
    Cargo.toml                    # Native Rust crate for action handlers
    README.md
    src/
      lib.rs                      # Kind registrations + handler wiring
      handlers.rs                 # Action handlers (create, update, move, comment, delete)
      state.rs                    # Task state enum + StateMachine config
    kinds/
      board.yaml
      list.yaml
      task.yaml
      comment.yaml
    ui/
      package.json                # @acme/project-ui — MF remote
      rsbuild.config.ts
      tsconfig.json
      src/
        index.ts                  # registerBlockComponent entry
        KanbanBoard.tsx           # The one custom component
        TaskCard.tsx              # Card sub-component
      dist/                       # Built output (gitignored)

clients/
  block-ui/
    package.json                  # @sys/block-ui
    tsconfig.json
    src/
      index.ts
      hooks/
        useAgentClient.ts
        useNode.ts
        useSlot.ts
        useNodes.ts
        useAction.ts
        useSubscription.ts
      components/
        BlockShell.tsx
        NodeLink.tsx
        SlotBadge.tsx
      registration.ts             # registerBlockComponent

crates/
  block-client/
    Cargo.toml
    src/
      lib.rs
      context.rs                  # BlockContext
      action.rs                   # ActionResult
      view_builder.rs             # ComponentTree builder helpers
      error.rs
    tests/
      harness.rs                  # test_harness utilities

  block-domain/
    Cargo.toml
    src/
      lib.rs
      state_machine.rs
      prioritised.rs
      assignment.rs
      tag_filter.rs
      slot_helpers.rs
      error.rs
```

## Relationship to SDUI milestones

This work exercises S5 (`/ui/render` + `KindManifest.views`) directly. It does **not** require S6 or S7 to be complete — no chart/sparkline/wizard used in the SDUI views. If S5's `views` field infrastructure isn't yet wired into the kind registry, that's the first dependency to resolve.

The `custom` Kanban widget exercises S3's `{type: "custom"}` variant + client-side registry, which is already shipped.

## Risks & mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| S5 `views` registration not complete | P1 blocks; SDUI views can't render from kind manifests | Fall back to authored `ui.page` nodes with `agent slots write` until S5 lands |
| `@sys/block-ui` hooks duplicate Studio internals | Maintenance burden, divergence | Extract from existing `frontend/src/sdui/useSubscriptions.ts` and `frontend/src/hooks/` — shared code, not parallel code |
| `view!{}` macro complexity | Scope creep on P0.2 | Mark as stretch; ship builder-pattern API first, macro later |
| Block Cargo crate outside workspace | Build complexity | Same pattern as existing `blocks/com.acme.hello/` — standalone `Cargo.toml`, not a workspace member |

## Decision log

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Three libraries, not one | Concerns are distinct: frontend UI hooks ≠ Rust client helpers ≠ domain patterns. Mixing them violates CODE-LAYOUT.md's "one responsibility per crate" rule. |
| 2 | `@sys/block-ui` is a separate package, not embedded in `@sys/studio` | Blocks must not depend on the full Studio app. The SDK is the minimal surface blocks import. Extraction from Studio internals is deliberate. |
| 3 | `block-client` wraps `agent-client`, doesn't replace it | Blocks that need the full client API use `agent-client` directly. `block-client` adds block-specific ergonomics (scoped context, action result enum). |
| 4 | `block-domain` does not include graph traversal | Graph traversal belongs in `graph`. `block-domain` holds domain patterns (state machines, priorities) that don't know the graph exists. |
| 5 | Kanban is the only MF widget | Proves the `custom` escape hatch end-to-end. Every other view (board overview, task detail, comments) is pure SDUI — proving the "zero frontend code" story. |
| 6 | Native Rust handler, not process block | Stage 3c (process supervisor) isn't shipped. The block links in-tree. When the supervisor lands, the same `NodeBehavior` impls move to a process binary with zero logic changes — that's the SDK's promise. |

## Success criteria

- [ ] A developer can create a new block, `cargo add block-client block-domain`, `pnpm add @sys/block-ui`, and have working SDUI views + action handlers + optional custom MF widget without reading Studio internals.
- [ ] The three libraries contain **zero** references to "project", "task", "board", or any domain-specific term.
- [ ] `com.acme.project` renders 4 SDUI views (board overview, task detail, task list, comment thread) with zero custom frontend code.
- [ ] The Kanban view uses only `@sys/block-ui` hooks — no direct `fetch()`, no manual subscription wiring, no Studio-internal imports.
- [ ] `make ci` passes with all three libraries + the example block in the build.
- [ ] Each library is under its line budget: `@sys/block-ui` < 1500 LOC, `block-client` < 800 LOC, `block-domain` < 1000 LOC.
