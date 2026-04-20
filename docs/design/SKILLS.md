# Skills & Knowledge Blocks

How the platform widens the block concept from "packaged Rust/Wasm code" to three content kinds behind one distribution pipe — and how a central block store lets users **pull** blocks into their own runtime to make their workflows smarter.

Companion docs:
- [BLOCKS.md](BLOCKS.md) — packaging, signing, manifest, scan lifecycle. Everything here is an *extension* of that model, not a parallel one.
- [MCP.md](MCP.md) — how skills and knowledge are exposed as MCP tools / resources / prompts.
- [RUNTIME.md](RUNTIME.md) — the flow engine that skills are authored against.
- [AI.md](AI.md) — how the AI runner consumes skills and knowledge via MCP.
- [USE-CASES.md](../usecase/USE-CASES.md) — the 24/7 AI coding system (UC3) and the central-MCP-service pattern that this design unlocks.

---

## The core idea

A **block** is the unit of distribution. Today it carries code (Rust / Wasm / gRPC process). We widen it to carry four payloads behind the same manifest, signature, and lifecycle:

| Block type | Payload | What it contributes at runtime | Isolation |
|---|---|---|---|
| **Code block** | Rust / Wasm / process binary | Native node kinds (drivers, integrations) | Wasm sandbox / gRPC child process |
| **Skill block** | A flow document (JSON) + manifest | A composed node kind + an MCP tool | Flow engine (inherits safe-state, audit, RBAC) |
| **Knowledge block** | Markdown, prompt templates, schemas | MCP resources + MCP prompts + Studio docs | None — read-only text |
| **MCP-client block** | Endpoint URL + auth config + manifest | Imported external tools appear as node kinds + re-exposed MCP tools | Process boundary of the remote MCP server |

The payloads differ; everything else (id namespace, `block.yaml` frontmatter, signing, version matcher, install/enable/disable/uninstall, per-tenant scoping, RBAC) is identical. The store doesn't know or care which payload it's shipping.

**One rule preserved from [BLOCKS.md](BLOCKS.md):** a block written today never has to be re-packaged. Skills and knowledge *add fields* to `block.yaml` — they don't fork the format.

---

## Why this shape

Three forces push on the same primitive:

1. **Users want to improve their system without writing Rust.** Code blocks require build toolchains, signing, Wasm targets. Skills and knowledge blocks don't.
2. **Teams have tacit conventions** — React component guides, incident runbooks, prompt styles — that AI flows should follow. Today those live in README files no workflow reads.
3. **Composition is the point.** A skill is literally a flow made of other blocks; shipping it is shipping a workflow *template* the next team can wire in. Without a central store, skills stay trapped in one tenant.

Unifying the three under one block model means one install UX, one trust story, one RBAC path, one audit surface.

---

## Block types — the three payloads

### Code block (existing)

Covered in [BLOCKS.md](BLOCKS.md). Ships Rust/Wasm/gRPC kinds. No change.

### Skill block — a flow published as a reusable unit

A skill is a flow document plus a manifest that elevates it to three surfaces:

```
┌────────────────────────────────────────────────────────────────────┐
│  blocks/com.acme.skill.triage_pr/                                  │
│    block.yaml            (type: skill)                             │
│    skill/                                                          │
│      flow.json           the crossflow diagram                     │
│      input_schema.json                                             │
│      output_schema.json                                            │
│      description.md      (static, signature-bound)                 │
│    signature                                                       │
└────────────────────────────────────────────────────────────────────┘
```

`block.yaml` adds one section:

```yaml
id: com.acme.skill.triage_pr
version: 1.2.0
type: skill                              # new discriminator — default: code

contributes:
  skill:
    flow: skill/flow.json
    input_schema:  skill/input_schema.json
    output_schema: skill/output_schema.json
    description_md: skill/description.md
    tier: read                           # read | write | destructive
    node_kind: com.acme.skill.triage_pr  # shows as this kind in the palette
    mcp_tool_id: triage_pr_v1            # shows as skills.triage_pr_v1 in MCP

requires:
  - { id: com.example.integration.github, range: "^2" }
  - { id: com.example.integration.slack,  range: "^1" }
  - { id: sys.ai.run,                     range: "^1" }
```

The three surfaces a skill lights up, all dispatched through the same engine call:

| Surface | What the user sees |
|---|---|
| **Palette node** | Draggable in the Studio flow editor; ports auto-generated from input/output schema. Skills compose into bigger skills. |
| **MCP tool** | `skills.<mcp_tool_id>` in `tools/list`, dispatched via `subflow_invoke` (see [MCP.md § Block- and node-contributed tools](MCP.md#block--and-node-contributed-tools)). |
| **Internal AI tool** | The local AI runner (`ai.run_cli`, `sys.ai.run`) sees skills as MCP tools automatically — per-user, same scope as any other block-contributed tool. |

All three resolve to `POST /api/v1/flows/<flow_id>/run` with the caller's `AuthContext`. REST ≡ MCP parity holds.

### MCP-client block — importing skills from external MCP servers

The fourth way skills enter the runtime: **we act as an MCP client** against somebody else's MCP server and import their tools as local skills. The external server might be Anthropic's filesystem server, a vendor's hosted service, another tenant's Control Plane, a colleague's laptop, or a public community MCP endpoint.

Once installed, an MCP-client block causes every remote tool to surface locally exactly like a native skill:

- Draggable in the Studio palette (under "MCP: &lt;server-name&gt;").
- Callable from flows (including from within other skill flows — composition).
- Re-exported through **our** MCP server to downstream consumers (the local AI runner, external Claude Desktop, etc.) — our runtime becomes a **federating hub**.

```
┌────────────────────────────────────────────────────────────────────┐
│  blocks/com.nube.mcp.vendor_x/                                     │
│    block.yaml            (type: mcp_client)                        │
│    mcp/                                                            │
│      connection.yaml     endpoint, transport, auth                 │
│      allowlist.yaml      optional — which remote tools to import   │
│      description.md                                                │
│    signature                                                       │
└────────────────────────────────────────────────────────────────────┘
```

```yaml
id: com.nube.mcp.vendor_x
version: 0.1.0
type: mcp_client

contributes:
  mcp_client:
    connection:
      transport: http         # http | stdio | sse
      url: https://mcp.vendor-x.com/mcp
      auth:
        kind: bearer
        token_ref: secrets://vendor_x_mcp_token    # resolved via the secret store
      # or transport: stdio with { command, args, env_ref } for local subprocess MCP servers
    import:
      tools: "*"              # or explicit list: [search_docs, create_ticket]
      resources: []           # default: import nothing; resources tend to be noisy
      prompts: []
    namespace_prefix: vendor_x  # remote "search_docs" → local "vendor_x.search_docs"
    rbac:
      default_tier: read      # remote tools imported at this tier by default
      overrides:
        create_ticket: write
        delete_ticket: destructive
    health:
      probe_interval: 60s
      timeout: 5s
```

#### How it works at runtime

```
          ┌────────────────────────────────────────────────────────┐
          │               OUR AGENT (the hub)                      │
          │                                                        │
          │   ┌──────────────────────────────────────────────────┐ │
          │   │  MCP server (outbound — what we expose)          │ │
          │   │    core tools + skills + node actions +          │ │
          │   │    federated tools (prefixed vendor_x.*)         │ │
          │   └──────────────────────────────────────────────────┘ │
          │                        ▲                               │
          │                        │  re-export                    │
          │   ┌──────────────────────────────────────────────────┐ │
          │   │  MCP-client supervisor                           │ │
          │   │    one client session per mcp_client block       │ │
          │   │    ▸ connects on enable, tears down on disable   │ │
          │   │    ▸ runs tools/list, caches schema              │ │
          │   │    ▸ registers imported tools in the local       │ │
          │   │      tool registry with dispatch.kind = mcp_fwd  │ │
          │   │    ▸ health-pings; marks Degraded on failure     │ │
          │   └──────────────────────────────────────────────────┘ │
          │                        │                               │
          └────────────────────────┼───────────────────────────────┘
                                   │  HTTP / stdio / SSE
                                   ▼
                    ╔══════════════════════════════╗
                    ║  Remote MCP server (vendor)  ║
                    ╚══════════════════════════════╝
```

A tool call on `vendor_x.search_docs` resolves as:

1. Caller hits the local MCP server (or drops a `vendor_x.search_docs` node in a flow).
2. Local dispatcher matches `dispatch.kind: mcp_forward` (a fifth dispatch kind, see [MCP.md](MCP.md)).
3. Supervisor forwards the call to the remote server over the live client session.
4. Response is validated against the cached remote `output_schema`, wrapped in our `AuthContext`, audited, and returned.

#### Why this matters

- **Add skills by adding a connection.** No Rust, no flow authoring, no packaging — point at a URL, install the block, get tools. The fastest possible path to "more capability."
- **Turn our runtime into an MCP federating hub.** A tenant's local AI runner sees one unified tool catalog — some local, some federated — without caring which side of the wire the implementation lives on.
- **Reuse the community.** Every existing MCP server ([filesystem](https://github.com/modelcontextprotocol/servers), GitHub, Slack, Postgres, Puppeteer, …) becomes installable as a block.

#### Security and safety

| Concern | How we handle it |
|---|---|
| **Remote server trust** | The MCP-client block's manifest is signed by the block author; it names the endpoint, but the *server itself* is external and untrusted. Admins approve each connection at install time (same "Trust this source?" dialog as installing any unsigned code). |
| **Tool description injection** | Remote `tools/list` descriptions are treated as **user-authored content**, not signed block content. Surfaced in Studio with a clear "imported from &lt;server&gt;" badge so operators don't confuse them with first-party tools. Never echoed into other tool descriptions. |
| **Result injection** | Federated tool results are structured-schema-validated (remote `output_schema`) the same way native block tools are. Unschema'd free-form prose → response rejected at the boundary. |
| **RBAC** | Federated tools inherit the local caller's `AuthContext`. The block's `rbac.default_tier` gates visibility; `overrides` promote specific tools to `write`/`destructive`. Remote server's own RBAC still applies end-to-end. |
| **Secrets** | `auth.token_ref` is resolved from the local secret store at call time. Tokens never appear in the block bundle or the graph. |
| **Name collisions** | `namespace_prefix` is mandatory; federated tool ids become `<prefix>.<remote_id>`. Two MCP-client blocks colliding on the same prefix fail scan. |
| **Network failure** | Federated tools show `available: false` in `tools/list` when the supervisor's health probe is red. Flows calling an unavailable tool error cleanly; they don't hang. |
| **Off-switch** | `mcp.federated_tools_enabled: false` kills all MCP-client-contributed tools without uninstalling the blocks — parallels `plugin_tools_enabled` in [MCP.md § Fourth off-switch layer](MCP.md#fourth-off-switch-layer). |

#### What you can do once it lands

- Install `com.nube.mcp.filesystem` — the local AI runner can now read/write files via a sandboxed MCP server running in a subprocess.
- Install `com.nube.mcp.github_official` — official GitHub MCP tools appear as `github.*` node kinds, usable in flows alongside our own GitHub block (or replacing it).
- Install `com.acme.mcp.internal_platform` — your employer's internal MCP service exposes `acme.deploy_service`, `acme.query_metrics`, and so on to every team member's runtime.
- Chain hubs: Tenant A's hub federates Tenant B's hub federates a vendor's MCP server. Every hop re-audits and re-authorises.

### Knowledge block — markdown, prompts, schemas

A knowledge block carries text that flows and AI consume. It's not passive documentation — every file is *addressable* and *versioned*.

```
┌────────────────────────────────────────────────────────────────────┐
│  blocks/com.nube.guide.react_components/                           │
│    block.yaml            (type: knowledge)                         │
│    content/                                                        │
│      guide.md            the main document                         │
│      examples/*.tsx                                                │
│    prompts/                                                        │
│      review_pr.md        parameterised prompt template             │
│      write_component.md                                            │
│    schemas/                                                        │
│      component.schema.json                                         │
│    signature                                                       │
└────────────────────────────────────────────────────────────────────┘
```

```yaml
id: com.nube.guide.react_components
version: 0.3.0
type: knowledge

contributes:
  knowledge:
    resources:
      - uri: "knowledge://react/guide"
        path: content/guide.md
        mime: text/markdown
      - uri: "knowledge://react/example/{name}"
        path_pattern: content/examples/{name}.tsx
        mime: text/tsx
    prompts:
      - id: review_react_pr
        template: prompts/review_pr.md
        params: [pr_diff, repo_conventions]
      - id: write_react_component
        template: prompts/write_component.md
        params: [brief]
    schemas:
      - id: component_spec
        path: schemas/component.schema.json
```

What each entry does at runtime:

| Entry | Consumed by |
|---|---|
| `resources` | MCP clients (`resources/read`) and Studio docs panel. Served via the same cache/subscription fabric as [MCP.md resources](MCP.md#resources). Immutable content → no TTL. |
| `prompts` | MCP clients (`prompts/get`) and the local AI runner. Parameters filled in at call time; the template itself is signature-bound. |
| `schemas` | JSON Schema references usable in other blocks' settings or in flow `validate` nodes. Enables cross-block type sharing. |

A knowledge block is never invoked — it's *read*. No RBAC tier (all read), no dispatch, no runtime cost beyond serving files. Auth still applies: private knowledge blocks in a tenant's org catalog are invisible to other tenants.

---

## The unified manifest

Every block — code, skill, or knowledge — uses the same `block.yaml` header. The `type:` field is the discriminator; `contributes:` carries one of three shapes:

```yaml
id: com.acme.<name>
version: 1.0.0
display_name: "..."
description: "..."
type: code | skill | knowledge | mcp_client   # default: code

contributes:
  ui:          { ... }                    # any type may ship a UI bundle
  kinds:       [ ... ]                    # code-only
  native_lib:  ...                        # code-only
  wasm_modules: [ ... ]                   # code-only
  process_bin: ...                        # code-only
  skill:       { ... }                    # skill-only (see above)
  knowledge:   { ... }                    # knowledge-only (see above)
  mcp_client:  { ... }                    # mcp_client-only (see above)
  mcp:         { tools, resources, prompts }   # any type may contribute directly

requires:
  - { id: <block-or-capability>, range: "..." }
```

Rules:
- **`type` gates `contributes.*`.** A `type: skill` block with `contributes.wasm_modules` is `Failed` at scan.
- **Namespace ownership** ([BLOCKS.md § Namespace ownership](BLOCKS.md#namespace-ownership)) applies uniformly. A skill block owns its id as a prefix for the node kind it contributes.
- **`requires:` is resolved at install.** Skill blocks declare the code/skill/knowledge blocks they compose over; install fails fast if the target runtime is missing them or has incompatible versions.

---

## The central block store

The store is a multi-tenant registry with a public catalog and a private per-org catalog. Same surface, scoped by auth.

```
                 ╔═══════════════════════════════════════╗
                 ║         CENTRAL BLOCK STORE           ║
                 ║  (part of Control Plane, cloud only)  ║
                 ║                                       ║
                 ║   ┌────────────────────────────────┐  ║
                 ║   │  Public catalog                │  ║
                 ║   │    code / skill / knowledge    │  ║
                 ║   │    anyone may pull             │  ║
                 ║   │    signed by verified authors  │  ║
                 ║   └────────────────────────────────┘  ║
                 ║   ┌────────────────────────────────┐  ║
                 ║   │  Org catalog  (per tenant)     │  ║
                 ║   │    internal skills, guides     │  ║
                 ║   │    only org members may pull   │  ║
                 ║   │    signed by org key           │  ║
                 ║   └────────────────────────────────┘  ║
                 ╚════════════════════╤══════════════════╝
                                      │
                                      │  HTTPS + JWT
                                      │  yourapp block install com.acme.skill.triage_pr@^1
                                      │
                     ┌────────────────┼────────────────┐
                     ▼                ▼                ▼
               ┌──────────┐     ┌──────────┐     ┌──────────┐
               │ edge     │     │ edge     │     │ dev      │
               │ agent    │     │ agent    │     │ laptop   │
               │ (site A) │     │ (site B) │     │ (solo)   │
               └──────────┘     └──────────┘     └──────────┘
```

### Store operations

| Operation | CLI | REST | What it does |
|---|---|---|---|
| Search | `yourapp block search <query> [--type skill]` | `GET /api/v1/store/search` | Full-text over id, description, tags. Filterable by type and capability. |
| Show | `yourapp block show <id>` | `GET /api/v1/store/blocks/<id>` | Manifest, available versions, author, signature chain, dependency tree. |
| Install | `yourapp block install <id>@<ver>` | `POST /api/v1/blocks/install` | Download → verify signature → resolve `requires:` → [scan](BLOCKS.md#loader-architecture) → register. |
| Update | `yourapp block update <id>` | `POST /api/v1/blocks/update` | Fetch newer satisfying version, rerun compat check, atomic swap. |
| Publish | `yourapp block publish ./my-block/` | `POST /api/v1/store/publish` | Org-scoped by default; public requires author verification. Rejects unsigned uploads. |
| Yank | `yourapp block yank <id>@<ver>` | `POST /api/v1/store/yank` | Marks a version as do-not-pull (keeps installed copies working). |

### Trust model

Same invariants as signed code blocks (per [BLOCKS.md](BLOCKS.md) Stage 10):

| Concern | How |
|---|---|
| Authenticity | Every block version is Ed25519-signed. The store records the signing key; the agent verifies before scan. |
| Author verification | Public catalog publishers go through a verification step (email + domain or GitHub org). Org catalog uses the tenant's own signing key. |
| Content tampering | Payload hash is part of the signature. A knowledge block's markdown can't be edited post-publish without invalidating the signature — same as Rust code. |
| Supply-chain transparency | Dependencies (`requires:`) resolve to exact versions at install time; an SBOM-style summary is recorded on the installing agent. |
| Per-tenant scoping | Private blocks are only visible to the authoring org. Public blocks can be hidden from a tenant via an admin deny-list. |
| Revocation | Yanked versions fail install; already-installed copies surface a warning but keep running (graceful degradation — see [RUNTIME.md](RUNTIME.md)). |

### What the store does NOT do

- **No remote execution.** The store serves signed bundles; everything runs locally on the user's agent. An installed skill runs on the user's flow engine under the user's RBAC. The store is a catalog, not a SaaS runtime.
- **No telemetry back to the store beyond install counts.** Privacy default.
- **No in-place editing.** Skills and knowledge are shipped as immutable signed bundles. Editing a skill means forking it in the Studio and publishing a new version.

---

## Composition — making workflows smarter by pulling blocks

The point of the three-layer model is that users improve their system by adding at any layer:

```
   ┌──────────────────────────────────────────────────────────────────┐
   │  KNOWLEDGE LAYER  — team conventions, guides, prompt templates  │
   │                    "how we do things"                            │
   └────────────────────────────┬─────────────────────────────────────┘
                                │  referenced by
                                ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │  SKILL LAYER      — reusable compositions over lower blocks     │
   │                    "the verbs we care about"                     │
   └────────────────────────────┬─────────────────────────────────────┘
                                │  composed from
                                ▼
   ┌──────────────────────────────────────────────────────────────────┐
   │  CODE LAYER       — raw integrations and protocol adapters      │
   │                    "the world's APIs"                            │
   └──────────────────────────────────────────────────────────────────┘
```

A worked example — nightly frontend PR review:

```
  flow: nightly_frontend_review
  ┌───────────────────────────────────────────────────────────────────┐
  │  [cron 22:00]                                                     │
  │       │                                                           │
  │       ▼                                                           │
  │  [github.list_prs]              ◀── CODE block                   │
  │       │                                                           │
  │       ▼                                                           │
  │  [foreach]                                                        │
  │       │                                                           │
  │       ▼                                                           │
  │  [skills.triage_pr_v1]          ◀── SKILL block                  │
  │       │                             internally consumes          │
  │       │                             knowledge://react/guide      │
  │       │                             as LLM system prompt         │
  │       ▼                                                           │
  │  [slack.post_review]             ◀── CODE block                  │
  └───────────────────────────────────────────────────────────────────┘
```

The user wrote **zero code**. They:

1. `block install com.example.integration.github`  (code)
2. `block install com.example.integration.slack`   (code)
3. `block install com.nube.guide.react_components` (knowledge)
4. `block install com.acme.skill.triage_pr`        (skill — pulls in its own deps)
5. Wired the four blocks together in the Studio.

Every installed block automatically:
- Shows in the Studio palette (code + skill) or docs panel (knowledge).
- Appears in the per-user MCP `tools/list` / `resources/list` / `prompts/list`.
- Becomes callable by the local AI runner without any extra config.

---

## Open-source foundations

We don't write a custom MCP stack. Where mature Rust crates exist, we adopt them and keep wrapper code thin.

| Concern | Crate | Role | Notes |
|---|---|---|---|
| MCP server + client | **[`rmcp`](https://github.com/modelcontextprotocol/rust-sdk)** (official Anthropic SDK, v1.5+) | Both sides. Independent Cargo features `server` and `client` — mirrors our gating exactly. | Pin exact version (`=1.5.x`) — exhaustive-struct lints mean non-breaking bumps can still break match arms. Disable default `reqwest` features and wire `rustls` explicitly to honour the no-OpenSSL rule. No WebSocket transport — don't plan for it. |
| MCP-client child-process transport | `rmcp` feature `transport-child-process` | Spawn `stdio` MCP servers as subprocesses (e.g. `npx @modelcontextprotocol/server-filesystem`). | This is the workhorse for the MCP-client block's `transport: stdio` mode. |
| MCP-client HTTP/SSE transport | `rmcp` features `transport-streamable-http-client` + `client-side-sse` | Remote HTTP-hosted MCP servers. | Streaming results flow back through the same client supervisor. |
| MCP server over HTTP | `rmcp` features `server` + `server-side-http` + `tower-service` | Our outbound MCP surface mounted under axum. | `tower-service` feature gives clean axum integration — no glue layer needed. |
| Signed block bundles (verify) | **[`minisign-verify`](https://github.com/jedisct1/rust-minisign-verify)** | Ed25519 signature verification on edge/cloud — tiny, audit-friendly, verify-only. | Preferred for the install-time path because it's intentionally minimal. |
| Signed block bundles (sign) | **`ed25519-dalek` v2** (from the [`curve25519-dalek`](https://github.com/dalek-cryptography/curve25519-dalek) monorepo) | Signing side in the Control Plane / publish path. | The old standalone `ed25519-dalek` repo is archived as of 2025-12 — use the monorepo location. |
| Remote tool schema validation | `schemars` + `serde_json` (already platform-standard per [OVERVIEW.md](OVERVIEW.md)) | Validate remote `output_schema` responses at the `mcp_forward` boundary. | Same libs the rest of the stack uses — no new dep. |

### Prior art we studied but don't depend on

- **[`nautilus-ops/mcp-center`](https://github.com/nautilus-ops/mcp-center)** — Rust MCP proxy/federator. Closest in shape to our federating hub, but low activity (last push 2025-09) and small community. Read the design, don't take the dep.
- TypeScript/Python MCP proxies (`mcp-proxy`, `supergateway`) — useful for understanding upstream-server quirks but irrelevant as libraries.

---

## Feature gating — MCP is optional on both sides

Neither the MCP server nor the MCP client is in the default build. Edge targets with a 180 MB RSS budget ([OVERVIEW.md § Memory budgets](OVERVIEW.md#memory-budgets)) and cloud compliance builds that reject untrusted agent protocols both get binaries with **zero MCP code**.

Two independent Cargo features, each pulling one side of `rmcp`:

| Feature | What compiles in | When to enable |
|---|---|---|
| `mcp-server` | `rmcp` with `server` + `server-side-http` + `transport-io`, our outbound tool/resource/prompt registries, the `POST /mcp` axum routes, audit wiring. | Cloud tenants exposing tools to LLM clients. Standalone dev laptops. Edge *only* when local Claude Desktop or CLI needs to drive the agent. |
| `mcp-client` | `rmcp` with `client` + `transport-child-process` + `transport-streamable-http-client` + `client-side-sse`, the MCP-client block supervisor, the `mcp_forward` dispatcher, health probes. | Any agent that should be able to install `mcp_client`-type blocks. Useful on dev laptops (install community MCP servers locally) and on cloud (federate internal enterprise MCP services). Usually off on constrained edge. |

Combinations:

| Build | Features | Result |
|---|---|---|
| Minimal edge (ARM 256 MB legacy) | *(neither)* | No MCP code at all. Cannot install `mcp_client` blocks; `type: mcp_client` entries at scan time fail cleanly with `Reason: feature mcp-client not compiled`. |
| Typical edge | *(neither)*, or `mcp-server` only for local admin | Engine runs, flows run, skills run as palette nodes. Federated skills unavailable. |
| Dev laptop | `mcp-server` + `mcp-client` | Everything. Can publish and consume. |
| Cloud, consumer role | `mcp-client` only | Federate upstream enterprise services into flows, but don't expose our own surface. |
| Cloud, producer role | `mcp-server` only | Expose our skills catalog to external LLM clients; no federation. |
| Cloud, full hub | `mcp-server` + `mcp-client` | Federating gateway — what the central-MCP-service use case in [USE-CASES.md](../usecase/USE-CASES.md) needs. |

This is the fifth off-switch layer, complementing the four in [MCP.md § The off-switch](MCP.md#the-off-switch-three-layers-defense-in-depth): compile-time (per-side), config (`mcp.enabled`, `mcp.client_enabled`), runtime (`yourapp mcp disable`, `yourapp mcp-client disable`), and existing per-surface toggles (`plugin_tools_enabled`, `federated_tools_enabled`). Each layer works independently — disabling a feature never requires rebuilding.

### What blocks behave like when a feature is absent

| Block type | `mcp-server` off | `mcp-client` off |
|---|---|---|
| `code` | Installs normally. Tools don't reach external LLMs. | Installs normally. |
| `skill` | Installs; usable as palette node; **not** exposed as MCP tool. | Installs normally. |
| `knowledge` | Installs; visible in Studio docs panel; **not** exposed as MCP resource/prompt. | Installs normally. |
| `mcp_client` | Installs; forwards locally into flows only (no re-export). | **Scan `Failed`** with a clear reason. Agent logs it; the rest of the system is unaffected. |

No runtime crashes from absent features — block status + structured reason is the contract.

---

## Runtime lifecycle — what happens on install

Extends [BLOCKS.md § Loader architecture](BLOCKS.md#loader-architecture). Three new branches in `BlockRegistry::scan`:

```
                    block.yaml
                        │
                        ▼
                 type = code ? ─── yes ──▶ existing code-block path
                        │                      (kinds / native / wasm / process)
                       no
                        │
                 type = skill ? ── yes ──▶ load flow.json
                        │                    validate against crossflow schema
                        │                    resolve `requires:` against installed blocks
                        │                    register as node kind with auto-generated ports
                        │                    register as MCP tool (dispatch.kind = subflow_invoke)
                       no
                        │
                 type = knowledge ? yes ─▶ index content/ for resource URIs
                        │                    load prompt templates
                        │                    register resources + prompts with MCP server
                        │                    serve via Studio docs panel
                        │
                  Failed (unknown type)
```

Every branch ends with a `BlockStatus::Enabled` node in the graph — or a structured `Failed` reason. Enable/disable/uninstall work uniformly across the three types.

### Update semantics

| Type | Update behaviour |
|---|---|
| Code | Restart required (Wasm reload is Stage-12; native needs agent restart). |
| Skill | Hot-swap: next invocation uses the new flow. In-flight runs finish on the old version. |
| Knowledge | Hot-swap: next `resources/read` returns the new content. Cached TTLs respected. |

---

## Versioning and compatibility

[VERSIONING.md](VERSIONING.md)'s capability matcher applies to all three block types. One difference worth flagging:

- **Skill blocks declare their transitive block deps** in `requires:`. A skill built against `github@2.x` won't install on a runtime with only `github@1.x`. The matcher runs before any flow JSON is loaded.
- **Knowledge blocks can declare soft deps** (`references:` instead of `requires:`) — a guide that *mentions* a kind but doesn't break if it's absent. Install succeeds with a warning.

---

## What the Studio shows

| Type | Where it surfaces |
|---|---|
| Code | Node palette (by category); block manager page (install/enable/disable); property-panel contributions. |
| Skill | Node palette (under "Skills" section); skill manager page (view flow, dependency tree, publish fork); appears in AI context picker. |
| Knowledge | Docs panel (searchable, openable inline); "Attach knowledge" chip on flow/node context; prompt picker in the AI chat's system-prompt composer. |

The context-aware AI chat ([AI.md § Global AI chat](AI.md#studio--the-user-facing-chat)) auto-includes relevant knowledge blocks based on the active route — e.g. editing a React page pulls in `knowledge://react/guide` as system prompt.

---

## Worked use cases

### 24/7 AI coding system (UC3)

From [USE-CASES.md § Use case 3](../usecase/USE-CASES.md#use-case-3--ai-developer-framework-scope-in-the-morning-ship-by-evening). The `dev.scope` node kind becomes a code block. The scope-execution flow becomes a **skill block** (`com.example.skill.execute_scope`) that any team can install. Team style guides ship as **knowledge blocks** (`com.example.guide.our_code_style`). New team members install three blocks and get the whole setup running.

### Business central MCP service

A company publishes an org catalog of skills — `skills.onboard_customer_v1`, `skills.refund_order_v1`, `skills.generate_quarterly_report_v1` — each one a flow that composes their internal integrations. External consumers (Claude Desktop, Cursor, internal LLM apps) connect to one MCP endpoint and see a curated verb catalog. The business never writes MCP server code; they author flows and publish them as skills.

### Solo developer productivity

Install `com.nube.guide.typescript_style`, `com.nube.guide.commit_messages`, and `com.acme.skill.draft_pr_description`. The local AI runner now writes PRs matching the team's conventions, using a skill that encodes the team's drafting workflow. All local, no SaaS, no custom code.

---

## What we're NOT doing

- **No execution sandboxing for skills beyond the flow engine.** Skills inherit the engine's safe-state, RBAC, and audit — that's the sandbox.
- **No skill-level secrets.** Skills reference secrets by name through the normal secret store; the skill block doesn't carry credentials. (Publishing a skill with embedded keys is a publish-time error.)
- **No AI training on the store.** The store is a catalog of blocks, not a dataset for model training.
- **No knowledge-block mutability at runtime.** If a team wants "living docs" that flows write to, that's a `notes` node kind (code block) with a `content` slot — not a knowledge block.
- **No fourth primitive.** Code / Skill / Knowledge covers distribution, composition, and content. A fifth category would be a design smell.

---

## Implementation staging

| Stage | What lands |
|---|---|
| **S0** | `type:` field added to `block.yaml` (default `code`); scan path unchanged. |
| **S1** | Skill block support: manifest, flow load, register as node kind, local install from file. |
| **S2** | Skill → MCP tool via `subflow_invoke` dispatch (see [MCP.md](MCP.md)). |
| **S3** | Knowledge block support: resources, prompts, Studio docs panel. |
| **S4** | MCP-client block support: connection supervisor, federated tool registration, `mcp_forward` dispatch, health probes, federation off-switch. |
| **S5** | Central store: search, install, update, publish, signing, org catalogs. |
| **S6** | Context-aware knowledge injection in the global AI chat. |
| **Post-v1** | Soft deps, knowledge-block search indexing, skill composition UI (visual "include skill" helper), skill versioning diff view. |

---

## One-line summary

**A block is any signed, versioned bundle — code, a flow, markdown, or an MCP-client connection — distributed through a central store, installed per-user per-tenant, and composed in the Studio so users make their workflows smarter by pulling blocks, not by writing code.**
