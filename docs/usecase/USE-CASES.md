# Three Use Cases

Concrete illustrations of what the platform can be used to build. None of them defines the platform — they're three very different applications of the same node / slot / flow / block substrate. If they all work without the platform bending, the generic thesis is real.

- **Use case 1** — a building automation / IoT platform with a live dashboard builder.
- **Use case 2** — a workflow automation platform where users bring their own AI accounts (Claude Code, OpenCode, etc.) and blocks orchestrate what the AI can reach.
- **Use case 3** — a developer-productivity framework: scope work in the morning, hand it to an AI, review progress via Slack. The platform builds software for you.

All three deploy on the same binary. None requires changes to the core. Each uses a different combination of first-party blocks plus its own domain-specific blocks.

---

## Use case 1 — BMS / IoT platform with a dashboard builder

### The scenario

A facilities company manages HVAC, lighting, and metering across dozens of buildings. Each site has a gateway that speaks BACnet and Modbus to local hardware. The company's operators need:

- Real-time visibility into every device on every site, live.
- Dashboards any ops engineer can build and share — no coding.
- Configurable control flows ("if outdoor temp > setpoint for 10 minutes, ramp chiller to stage 2") deployed and supervised remotely.
- Alarms that route to email, SMS, or a ticketing system.
- Fleet-level operations — deploy a schedule change to 40 sites at once, roll back if any fail.
- Per-customer isolation: each building owner sees only their devices.

### How the platform carries it

Everything is already in the core. No platform changes. The company ships four domain blocks and composes them with what the platform gives for free.

#### Blocks they ship

| Block | What it contributes | Platform facility used |
|---|---|---|
| `com.acmefac.driver.bacnet` | `bacnet.network` / `bacnet.device` / `bacnet.point` kinds with placement rules, discovery, COV subscriptions, priority-array writes | Block process over gRPC-UDS; kind registration; settings schemas (one variant for IP, one for MSTP) |
| `com.acmefac.driver.modbus` | `modbus.network` / `modbus.device` / `modbus.register` kinds | Multi-variant settings (serial RTU vs TCP) — see NODE-AUTHORING.md Modbus example |
| `com.acmefac.ui.dashboard` | Widget node kinds: gauge, trend, value tile, setpoint control, schedule editor, floor-plan overlay | Module Federation UI bundle + schema-driven property panels |
| `com.acmefac.alarm.router` | `alarm.rule` node kind with conditions and routing (email, SMS, webhook) | Slot subscription over NATS; reuse of the generic query layer |

#### The graph for one building

```
acme-site-1  (station)
├─ floor-1  (folder)
│  ├─ ahu-1  (bacnet.device)
│  │   ├─ supply-temp  (bacnet.point, isPoint, isWritable=false)
│  │   ├─ setpoint     (bacnet.point, isWritable=true, safe_state=release)
│  │   └─ fan-command  (bacnet.point, isWritable=true, safe_state=fail_safe)
│  ├─ vav-3-21  (bacnet.device) ...
│  └─ energy-meter-1  (modbus.device) ...
├─ dashboards  (folder)
│  ├─ operations-overview  (dashboard)
│  │   ├─ outdoor-temp-gauge  (ui.widget.gauge → /acme-site-1/weather/outdoor-temp.value)
│  │   ├─ ahu-status-tile    (ui.widget.value  → /acme-site-1/floor-1/ahu-1/fan-command.value)
│  │   └─ energy-trend        (ui.widget.trend → /acme-site-1/floor-1/energy-meter-1/kwh.value, range=24h)
│  └─ chiller-detail          (dashboard) ...
├─ flows  (folder)
│  ├─ chiller-stage-logic  (flow)
│  └─ overnight-setback    (flow)
└─ alarms  (folder)
   └─ critical-temp-rule  (alarm.rule)
```

Everything in that tree is a node in the same graph, by the same rules:

- The device hierarchy is enforced by containment (`bacnet.point` only lives under `bacnet.device`).
- Each dashboard is itself a node — persisted, permissioned, versioned, exported, imported, cascading-deleted like anything else.
- A widget references a source slot by path; it subscribes over NATS (`graph.<tenant>.<path>.slot.<slot>.changed`) and re-renders live without the dashboard code knowing or caring what device it is.
- Flows are children of a `flows` folder. Each flow is a container node whose children are compute/logic/integration nodes with wires between their slots. Live-wire mode is used for trivial logic (single setpoint follows a schedule); the flow-document mode is used when the logic is large enough to warrant a named, pauseable unit.
- Alarms are nodes too. An `alarm.rule` subscribes to a slot, evaluates a condition, and when it fires, emits to another node — the router — which turns it into an email/SMS/webhook. The audit log captures every fire and every delivery.

#### Dashboards — how they're built and how they work

The dashboard builder isn't a separate app; it's the Studio in dashboard-edit mode.

1. User creates a `dashboard` node in the graph — just like any other node. Placement is wherever they're allowed.
2. In edit mode, Studio shows the widget palette (populated from every kind facet-tagged `isWidget`).
3. User drags a gauge onto the canvas. Property panel (rendered by `@rjsf/core` from the widget's settings schema) asks: which slot? what range? what units? what thresholds for colour bands?
4. On save, the dashboard is a node with children — widget nodes whose `config` slot holds their bound path, range, colour config, etc.
5. At view time, each widget opens a NATS subscription on its bound path's slot subject. Values stream in; the widget re-renders. When a user edits the dashboard, it's just a graph mutation — audit log, permissions, and cascading delete all come for free.

Sharing a dashboard across sites: export the dashboard subtree as JSON (same format as flows), import it into another tenant's tree, remap any hard-coded paths. Because dashboards are nodes, the standard graph API handles export/import with no new endpoint.

#### Flows — the HVAC logic

A "chiller stage logic" flow lives as a node in the graph. Authored on the canvas:

```
[schedule.at("06:00")] ──▶ [read-slot /acme-site-1/weather/outdoor-temp]
                                        │
                                        ▼
                           [compute: temp > 28 && hour ∈ 8..18]
                                        │
                              true ─────┴───── false
                                │               │
                                ▼               ▼
             [write-slot chiller-stage=2]  [write-slot chiller-stage=1]
```

The flow reads and writes slots the same way a device driver does — no special "control" API exists; everything is slot I/O on a typed graph. Safe-state on flow stop is declared per writable output (`chiller-stage` has `fail_safe=1` so if the flow crashes, stage defaults to 1, not 0-or-whatever).

Commissioning mode lets operators dry-run the flow with synthetic outdoor temps; simulation mode does not write to any point. Both are the platform's built-in operational modes from RUNTIME.md — the ops team didn't build them.

#### Fleet management

Because every site is an edge agent talking to one Control Plane, the "deploy to 40 sites" story is the platform's fleet orchestration:

- Each site's agent is a node in the Control Plane's graph.
- A rollout is a command (via the MCP tool `deploy_flow` or the `yourapp flow deploy` CLI) that targets a subtree (`tenant.sys.sites.*`) and applies the new flow/dashboard/alarm-rule with canary → staged → fleet-wide policy.
- Rollback is one command because every version is retained.
- NATS scatter-gather handles "firmware version on every gateway in city X" as one request with a wildcard subject and a timeout.

#### Multi-tenancy

Cloud is multi-tenant. Each building owner is one Zitadel org. Tenant IDs are baked into NATS subjects and Postgres row-level security. A building owner who opens Studio sees their own tree and no one else's. Per OVERVIEW.md edge is single-tenant in v1, so each gateway belongs to one customer — the facilities company ships one agent per site per customer.

#### What the company did NOT have to build

- The JWT/JWKS chain and offline verification.
- The per-site local database and outbox.
- The deployment hierarchy / rollout engine.
- The alarm-event audit stream.
- The live WebSocket fabric to the dashboards.
- The multi-tenant subject taxonomy.
- The property-panel form renderer.
- The three-way variant-settings UI for Modbus RTU vs TCP.
- Block isolation, crash recovery, signed install.

All of that came from the platform. The company shipped four blocks and a domain model.

---

## Use case 2 — AI workflow automation using the user's own AI account

### The scenario

A small engineering team wants to automate the boring parts of their work: triaging issues, reviewing PRs, drafting release notes, running nightly codebase checks, wiring alerts to Slack. They already pay for Claude Code (or OpenCode, or a Copilot subscription, or an Ollama server). They don't want to pay a separate AI bill to a SaaS — they want a platform that **orchestrates** work across their existing tools while using the AI credits they already have.

Needs:

- Install blocks that connect to the services they already use (GitHub, Linear, Slack, S3, internal APIs).
- Author flows that combine those blocks with AI reasoning steps.
- AI steps execute via the user's existing Claude Code (or other provider) session on the same machine — no new billing relationship.
- The AI, when invoked, can reach into any installed block as a tool — via MCP.
- Flows run on a schedule, a webhook, a Slack command, or manually.
- Team members can install/uninstall blocks on themselves without breaking anyone else's flows.

### How the platform carries it

Everything is already in the core. The team installs a few domain blocks. The AI-account integration is a single specialist block category.

#### Blocks that make this work

| Block | What it contributes |
|---|---|
| `com.example.ai.claude_code_runner` | A `ai.run_cli` node kind. Spawns the user's locally-installed `claude` CLI as a subprocess; feeds it a prompt plus a temporary MCP config pointing at this agent's per-user MCP endpoint; streams events back. |
| `com.example.ai.opencode_runner` | Same shape, different CLI. Users with OpenCode installed choose this runner. |
| `com.example.ai.ollama_runner` | HTTP to a local or LAN Ollama server — for users who'd rather stay off any hosted API. |
| `com.example.integration.github` | Nodes for `github.list_prs`, `github.get_pr_diff`, `github.comment_pr`, `github.create_issue`. Auth via a per-user GitHub token stored in the node's config slot. |
| `com.example.integration.linear` | Nodes for Linear tickets. |
| `com.example.integration.slack` | Nodes for sending messages, reading channels, handling slash-command triggers. |

None of these are special. They're all blocks the same way a BACnet driver is an block. What makes the AI use case work is the MCP endpoint the platform ships already.

#### The MCP story — the whole thing rests on this

The platform's [MCP server](../design/MCP.md) is already per-user: an authenticated MCP client connects to `/mcp` and gets a scoped tool surface built from that user's installed blocks plus the public slot API. Tools are filtered by the user's RBAC, audited per call, and safely presented to the LLM.

This means **the AI sees the user's installed blocks as tools, the moment they install them.** No intermediate app-tool packaging layer is needed on top — blocks already contribute node kinds, and node kinds already contribute their operations to the MCP surface through the generic path.

When a flow node of kind `ai.run_cli` fires, it:

1. Generates a temporary MCP config file pointing at `http://localhost:<agent-port>/mcp` with a short-lived user token.
2. Spawns `claude --mcp-config <file>` (or `opencode`, or whichever runner was configured) with the prompt from `msg.payload`.
3. Streams the Claude Code CLI's stdout (structured JSON events) back through its output slot as a series of `Msg`es. Each emitted msg is `msg.child(...)`'d from the triggering input, so the trace context on the transport links the stream back to the prompt that kicked it off.
4. Lets the user's Claude Code session drive the conversation — including calling any tool exposed via MCP, which is every installed block.

Because the CLI runs locally and uses the user's own credentials:

- The AI bill lives with the user's existing subscription. The platform adds nothing to it.
- The user's local AI session-history and context-caching are preserved.
- Enterprise policies that already govern what Claude Code can touch on that machine still apply.
- Ollama users keep their inference local — nothing leaves the LAN.

#### An example flow — nightly PR review

```
[cron "0 22 * * 1-5"]
       │
       ▼
[github.list_prs (state=open, since=24h)]            ◀── msg.state="open" hardcoded; msg.since from trigger
       │
       ▼
[foreach]                                            ◀── fans out one msg per PR
       │
       ▼
[github.get_pr_diff (pr_number from msg.payload)]
       │
       ▼
[ai.run_cli (runner=claude, prompt-template)]        ◀── prompt asks Claude Code to review the diff
       │                                              using the installed MCP tools
       ▼
[slack.send_message (channel=#code-review)]          ◀── posts Claude's review summary + PR link
```

The `ai.run_cli` node's prompt template says "review this PR, check for X/Y/Z, and if you spot anything urgent, call the `github.comment_pr` tool to leave a line-specific comment." Claude Code — running as the user — does exactly that via the MCP endpoint. No glue code; the platform already routed `github.comment_pr` from the GitHub block through the MCP surface to the CLI.

#### Another — Slack slash command → Claude → action

```
[slack.slash_command "/summarize-linear"]   (webhook trigger node from Slack block)
       │
       ▼
[linear.list_issues (query from msg.payload.text)]
       │
       ▼
[ai.run_cli (summarise these issues, post back to the Slack thread)]
       │
       ▼
[slack.reply_in_thread (thread_ts from msg.payload)]
```

Same pattern. AI runs as the user. Tools are the installed blocks. Platform is the orchestration layer.

#### Settings and msg overrides — why authoring is easy

The pattern from [NODE-AUTHORING.md](../design/NODE-AUTHORING.md) applies throughout:

- The `github.list_prs` kind declares a settings schema with a default owner/repo/state. Flow authors drop the node, type a repo, done.
- When they want per-message dynamism, they set `msg.owner`, `msg.repo`, `msg.state` upstream. The engine resolves msg over config over schema defaults.
- The `ai.run_cli` kind allows `msg.prompt` to override the static prompt template, and `msg.model` / `msg.system_prompt` to override those respectively. Security-sensitive fields (the MCP token, the CLI path on disk) are NOT overridable — they live in config only, enforced by the manifest omitting them from `msg_overrides`.

This is the difference between "build your own integration scaffolding" and "write the specific thing you need." The team adds a GitHub block in an afternoon, not a week.

#### Multi-provider, one flow shape

The three AI-runner blocks (Claude Code, OpenCode, Ollama) contribute different kinds but share the same shape: input is a `Msg` with a prompt and optional overrides, output is a stream of AI events ending in a final result. Flow authors can swap the runner node without rewiring anything else. The user picks based on cost, privacy, and what they already pay for.

#### Memory and context — not in the platform

Session memory (what the AI "remembers" across runs) isn't a platform feature. The user's Claude Code / OpenCode session already handles conversational continuity locally. If a team wants shared context across flows, they do it the normal way: a `team-notes.md` node whose `content` slot is read into the prompt template of `ai.run_cli`. No new primitive needed; it's just another node.

#### Per-user scoping

MCP tool visibility is scoped to the authenticated user's installed blocks. Alice installs GitHub + Linear + Claude runner; Bob installs Slack + OpenCode runner. Alice's flows and AI sessions see only her tools. Bob's see only his. Everything is per-user because that's already how the graph, the MCP endpoint, and the auth layer work.

#### What the team did NOT have to build

- The MCP server, per-user scoping, and the auth/token dance.
- The block install/update/uninstall pipeline with capability-based compat checks.
- The flow engine with cron / webhook / manual triggers and approval gates.
- The JSON-Schema-driven settings forms and multi-variant UI (e.g. "which AI runner do you want to use on this node?" — that's just a multi-variant settings schema on the `ai.run_cli` kind).
- Audit, per-call observability, rate limits, error routing.
- Fan-out / fan-in semantics so `foreach` over PRs works correctly.

All of that came from the platform. The team shipped a handful of integration blocks plus their choice of AI-runner block.

---

---

## Use case 3 — AI developer framework: scope in the morning, ship by evening

### The scenario

A developer — solo or on a small team — wants to stop hand-crafting every feature. Mornings are for thinking: write scopes, break work into stages, define acceptance criteria. The rest of the day, an AI works through the scopes on their behalf, pinging the developer in Slack at key checkpoints so they can review, redirect, or approve the next stage.

Concretely:

- At 9 AM the developer opens the Studio (or a CLI) and writes five scopes — small, well-defined units of work. Each has a title, description, stage plan, acceptance criteria, and target repo.
- Scopes get saved. Kicking one off starts a flow: AI picks up the scope, works through stage 1, opens a PR, posts a Slack message — "Stage 1 done on scope X, PR #42 up for review."
- The developer reviews in Slack: approve, ask for changes, reject. If approved, the flow advances to stage 2. If changes requested, the AI iterates on the same PR with the feedback. If rejected, the scope pauses.
- By evening, half the scopes are shipped, one is blocked on the developer's input, one is still in stage 3. The developer never opened a code editor — they wrote scopes and reviewed PRs.

This is the platform **building software for its own team**, and eventually for itself.

### How the platform carries it

Reuse everything from use case 2 (AI runners, GitHub/Slack integrations, MCP per user). Add one new block and one flow pattern.

#### The new block

| Block | What it contributes |
|---|---|
| `com.example.dev.scope` | A `scope` node kind with rich metadata; a `scope-plan` container kind; MCP tools for the AI to read/update scopes; a Studio tab for authoring scopes; CLI commands for the developer |

A scope is a node. Its `config` slots are the authored content (title, description, stages, acceptance criteria, target repo). Its `status` slots are runtime state (current stage, PR links, last AI message, review state). Nothing about this kind is special to the platform — it's a domain block following the same rules as a BACnet driver or a dashboard widget.

```yaml
kind: com.example.dev.scope
facets: [isContainer, isWorkItem]
must_live_under: [com.example.dev.scope_plan, sys.core.folder]
may_contain: [com.example.dev.scope_note, com.example.dev.scope_artefact]

slots:
  config:
    title:                { type: string, required: true }
    description:          { type: string, format: markdown }
    target_repo:          { type: string }                    # owner/repo
    target_branch:        { type: string, default: "main" }
    stages:               { type: array, items: { $ref: "#/defs/stage" } }
    acceptance_criteria:  { type: array, items: string }
    priority:             { type: string, enum: [low, med, high], default: med }
    runner:               { type: string, default: "claude-code" }  # which ai runner kind to spawn
  status:
    current_stage:        { type: integer, default: 0 }
    state:                { type: string, enum: [draft, queued, running, awaiting_review, blocked, done, cancelled] }
    pr_url:               { type: string }
    branch:               { type: string }
    last_ai_message:      { type: string }
    review_thread_ts:     { type: string }    # Slack thread for this scope's reviews
    started_at:           { type: integer }
    finished_at:          { type: integer }

settings_schema:
  # the same fields as config, rendered as a form for the morning authoring flow
  ...
```

The scope's `status.state` is the state machine. Every transition fires `graph.<tenant>.<path>.lifecycle.<from>.<to>` on NATS — which is what the flow engine, the Slack notifier, and any subscribed dashboard react to.

#### The graph for the developer

```
dev-workspace  (station)
├─ scope-plans  (folder)
│  ├─ today                (scope_plan, dated 2026-04-19)
│  │   ├─ scope-1-auth-refactor     (scope — state=done, pr=#42)
│  │   ├─ scope-2-migration-script  (scope — state=awaiting_review, pr=#43)
│  │   ├─ scope-3-flaky-test-fix    (scope — state=running, current_stage=2)
│  │   ├─ scope-4-readme-cleanup    (scope — state=blocked)
│  │   └─ scope-5-api-v2-endpoint   (scope — state=queued)
│  ├─ 2026-04-18           (scope_plan)
│  └─ backlog              (scope_plan)
├─ flows  (folder)
│  └─ scope-execution      (flow — the orchestration flow, generic, parameterised by scope path)
└─ integrations  (folder)
   ├─ github                (from use case 2)
   ├─ slack                 (from use case 2)
   └─ claude-code           (from use case 2 — the ai runner)
```

Scopes sit in a daily scope-plan. Running a scope means starting the `scope-execution` flow with that scope's path as input.

#### The morning — authoring scopes

The developer opens Studio to the "Scopes" tab (contributed by the `dev.scope` block's UI bundle). A scope-plan for today is created if none exists. For each scope:

- Title + description: free-form, supports markdown.
- **Stage plan**: a list of named stages ("scaffolding", "core logic", "tests", "docs"). Each stage has its own short description.
- Acceptance criteria: checkboxes.
- Target repo/branch.
- Which AI runner kind to use (Claude Code, OpenCode, local Ollama) — pulled from a dropdown populated by facet-filtering every node kind with `isAIRunner: true`.

Because scope authoring is just "edit this node's config slots", the form is rendered entirely by `@rjsf/core` from the kind's settings schema. No custom authoring UI beyond the tab shell. Scopes can also be authored from the CLI (`yourapp scope new --plan today --from-file ./scope.md`) or via MCP by another AI, if the developer wants an AI helping them plan.

When the developer is done, they select scopes and hit **Queue**. This writes `status.state = queued` on each selected scope, which fires a `lifecycle.draft.queued` event. A single running instance of the `scope-execution` flow picks up any queued scope — no per-scope flow instances.

#### The execution flow — one flow, all scopes

The `scope-execution` flow is a container `sys.core.flow` node in the graph. Its topology:

```
 [subscribe: graph.*.scopes.**.lifecycle.*.queued]
                       │
                       ▼
            [dev.scope.lock (path from msg)]          ◀── transitions state=queued → running
                       │
                       ▼
            [dev.scope.render_prompt (path from msg, stage=1)]
                       │                              ◀── reads scope config + stage plan + repo context,
                       ▼                                   produces the initial prompt for the AI
      [ai.run_cli (runner from scope.config.runner)]
                       │                              ◀── streams AI events back; final output is summary + PR url
                       ▼
           [dev.scope.record_progress]                ◀── writes pr_url, last_ai_message, current_stage
                       │
                       ▼
           [slack.send_message                        ◀── "[scope-3] stage-1 done, PR #43 open,
             (channel=#ai-dev, thread_ts=                  approve to continue to stage 2"
              scope.status.review_thread_ts)]
                       │
                       ▼
               [approval-gate (timeout=24h)]          ◀── flow parks; Slack reactions / slash commands
                       │                                   resolve it via the approval API
           approved ───┴──── rejected ──── changes
              │                 │               │
              ▼                 ▼               ▼
       advance stage        mark blocked    feed comments
      (loop back to                          back to AI runner
       render_prompt                         and re-run same stage
       with stage+1)
```

Everything in that flow is composed of nodes the platform already gives you. The only domain-specific nodes are `dev.scope.lock`, `dev.scope.render_prompt`, `dev.scope.record_progress` — and those are thin adapters over slot reads/writes on the scope node. The approval gate, the subscribe-to-NATS trigger, the AI runner, the Slack send are all reused.

When `current_stage` exceeds `len(stages)`, the flow writes `state = done`, posts a final Slack message, and exits.

#### Slack — the human-in-the-loop surface

The developer's primary interface during execution is Slack, not the Studio. The `slack` block contributes:

- **Notifications** — "[scope-3] stage-1 done, PR #43, approve to continue" with inline buttons (Approve / Request Changes / Reject). Buttons hit the platform's approval API.
- **Slash commands** — `/scope list`, `/scope pause scope-3`, `/scope comment scope-3 "rename the function"`. The comment command routes into the approval gate's `changes` branch, feeding the text back to the AI runner as additional context for the next iteration.
- **Per-scope threads** — every scope has a Slack thread. Every AI checkpoint posts to that thread. If the developer replies in the thread, a `slack` inbound webhook turns the reply into a scope comment without them needing to know any commands.

The platform already handles the substrate: Slack is an block, threads are string IDs stored on slots, buttons are approval-gate decisions, inbound slash commands are webhook triggers. Nothing about this is a Slack-specific primitive in the core.

#### The AI's view

The AI runner (Claude Code, OpenCode, whatever) runs locally as the developer's user — per use case 2, the platform adds nothing to the AI bill. When it's invoked for stage N of a scope, its prompt includes:

- The scope's full config (description, stages, acceptance criteria).
- The current stage's description.
- A pointer to the target repo and branch.
- The last AI message and any developer comments from the Slack thread.
- A list of available tools via MCP — `github.*`, `scope.read_scope`, `scope.append_note`, and anything else the developer has installed.

The AI can read back its own scope, check previous stages' notes, open/update PRs, leave comments, fetch diffs. If it needs the developer, it writes a note (`scope.append_note(scope_path, "need clarification on X")`), finishes the stage output, and the flow posts the note to Slack — so the developer sees "hey, I need you to clarify X" in the same thread they've been watching.

#### Iteration

The "request changes" path is the important one. When the developer hits the button in Slack:

1. Approval gate resolves to `changes` with the developer's comment as the payload.
2. Flow loops back to `render_prompt` with `stage` unchanged but `extra_context` carrying the feedback.
3. AI runner re-fires with "your last output is PR #43. The developer says: <comment>. Revise."
4. AI amends the PR (`git commit --amend` + `github.update_pr` via the GitHub block).
5. New Slack message: "[scope-3] stage-1 revised, same PR — approve?"
6. Cycle repeats until approved, rejected, or timed-out.

Multiple revision cycles are just more loop iterations. The flow doesn't need a special "revision" concept; it's approval gates plus a back-edge.

#### Morning scoping, evening dashboard

A separate dashboard shows the developer how today went — straight from the same `dashboard` kind used in use case 1:

- Tiles bound to slots on each scope: state, stage, PR URL, elapsed time.
- A trend of "stages completed per hour" drawn from `status.current_stage` timestamps.
- A "blocked scopes" section that filters by `state=blocked`.

Because scopes are nodes and dashboards are nodes over slot subscriptions, the dashboard is one import away from working. No bespoke reporting system.

#### Recursive — using the platform to build the platform

The platform's own `/crates/*` work can be scoped and executed this way. "Implement Stage 5 persistence" becomes a scope. "Fix the clippy warnings in `crates/graph`" becomes a scope. The developer writes scopes in the morning, AI chips through them using the developer's own Claude Code account, Slack mediates review, PRs land against the platform repo. The platform builds itself.

This use case isn't hypothetical — once a team has the blocks from use case 2 and the `dev.scope` block, they can drop their current IDE-driven workflow for this one.

#### What the developer did NOT have to build

- Persistent scope storage, versioning, audit trail — scopes are nodes.
- The multi-stage approval state machine — flow engine + approval gates.
- Slack threading, buttons, slash commands, inbound webhooks — Slack block + trigger nodes.
- The AI invocation, prompt resolution, msg overrides — AI-runner block plus the authoring pattern from NODE-AUTHORING.
- MCP tool serving so the AI can reach back into scopes and GitHub — built-in, per-user.
- The dashboard for tracking today's progress — dashboard kind + widget block.
- Error handling, retry, scope pausing on failure — flow engine error policies.
- Cross-scope coordination, mutual exclusion on a scope in flight — single flow instance + scope locking via `status.state`.

Everything that ships as platform-provided is reused. The developer wrote one block (`dev.scope`) and a single flow definition. The rest is authoring scopes and reviewing in Slack.

---

## What the three use cases have in common

None of the three use cases is the platform. They share a substrate:

| Substrate | How each use case uses it |
|---|---|
| **Everything is a node** | BMS devices / dashboards / flows (UC1). AI tasks / integrations / prompts (UC2). Scopes / stages / PR links (UC3). Same tree, same rules. |
| **Typed slots + wires** | BACnet point values stream into dashboard widgets and HVAC flows. PR diffs and AI prompts stream through AI-runner nodes. Scope state transitions drive flow branches. Same `Msg` envelope. |
| **Node-RED-compatible msg** | Ops engineers familiar with Node-RED port their logic. AI workflow authors port theirs. Developers writing flows to orchestrate their own work do the same. Same shape on the wire. |
| **Blocks** | BACnet / Modbus drivers (UC1). GitHub / Linear / Slack / AI-runner blocks (UC2). Same three-layer model. UC3 reuses the UC2 blocks verbatim and adds one: `dev.scope`. |
| **Capability manifests + install-time compat** | Site upgrades don't silently break driver blocks. Team upgrades don't break an AI flow. Platform upgrades don't break scope execution because the scope block declares what it needs. Same matcher across all three. |
| **MCP endpoint per user** | Optional in UC1. Essential in UC2 and UC3 — in UC3 the AI reaches back into scope status, GitHub PRs, Slack threads, and more through the same per-user scoped surface. |
| **Flow engine with approval gates** | HVAC logic (UC1). Multi-step workflows with approval (UC2). The "ping-Slack-and-wait-for-developer" pattern is just an approval gate with a 24h timeout (UC3). |
| **Fleet orchestration** | 40-site rollout (UC1). "Install the new Slack block on every team member's agent" (UC2). "Run this developer's scope-execution flow on a dedicated agent per team member" (UC3). Same primitives. |
| **Safe-state + commissioning + simulation modes** | Chiller setpoints don't get written in simulation. AI flow dry-runs don't actually comment on PRs. Scope flows in simulation don't open real PRs or send real Slack messages — useful for testing flow changes without billing AI tokens or spamming the team channel. |
| **Dashboards as nodes** | Live plant view (UC1). Team activity (UC2, if wanted). Today's scope progress (UC3). Same `dashboard` + widget kinds. |
| **Multi-tenant + per-user RBAC** | Each building owner sees only their site. Each teammate sees only their installed tools. Each developer sees only their scopes. Same RBAC. |

If the platform is generic enough that a BAS company, an AI-ops team, and a developer shop can all ship products on it without the core bending, the generic thesis holds. These three use cases are the smoke test.

Use case 3 is the one we eat our own dog food on — the platform's own development work can be scoped, executed, and reviewed the same way. Building a platform that builds itself is the compounding advantage.

## Not described here

A lot of applications fit — home automation, ETL pipelines, internal tool orchestration, service glue, workflow engines for non-AI business processes, IoT telemetry, dashboards for any domain that has telemetry. None of them are the platform. They're all applications of it. This doc picks two to make the shape concrete.
