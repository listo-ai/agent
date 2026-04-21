# System Bootstrap

First-boot setup and provisioning for cloud and edge agents — ensuring
that neither role starts in an open, unconfigured state.

Companion docs:
- [AUTH.md](AUTH.md) — identity model, Zitadel, JWT verification, RBAC.
- [OVERVIEW.md](OVERVIEW.md) — deployment profiles and the role model.
- [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md) — edge↔cloud connection layer.
- [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md) — why setup state is a graph node.

---

## The problem

Today every agent role — cloud, edge, standalone — boots with
`AuthConfig::DevNull`. Every request passes as admin on the default tenant.
This is correct for standalone (a dev/appliance role) but wrong for cloud
and edge:

- **Cloud** is a multi-user, remotely-reachable control plane. It must
  require auth from first boot. There is currently no path to bootstrap the
  first Zitadel org and admin account.
- **Edge** in production needs either local auth (a strong token, persisted
  to SQLite) or cloud-enrolled auth (a Zitadel service account). Today it
  boots open. An operator has no structured way to say "this edge is not yet
  provisioned" or to join it to a cloud tenant.

The guard documented in `config/src/model.rs` (`role=cloud + DevNull →
refuse boot in release`) was never implemented.

---

## Goals

1. Cloud and edge must **never boot in a fully-open state** once this
   ships.
2. A **first-boot operator flow** exists for both roles: structured, minimal,
   and completable without a running Zitadel instance already configured.
3. **Setup state is visible in the graph** — `sys.auth.setup` node — so
   Studio and flows can react to it the same way they react to any other
   system state.
4. Basic **edge enrollment** with a cloud is possible (opertor-initiated,
   not automatic).
5. All of this is **additive** — standalone + existing config-file consumers
   are unchanged.

---

## Non-goals / explicitly out of scope

- Full Zitadel provider (`ZitadelProvider` / JWKS fetch + offline verify).
  The Zitadel crate is listed as a follow-on stage (see §Phases below).
  This scope lands the setup endpoints and the config shape; the actual
  production JWT verification is the next stage.
- Any Studio UI for setup. The setup flow is an API and CLI surface only.
  Studio can call the same endpoints; the pages are a separate landing.
- Automatic edge re-enrollment, zero-touch provisioning, or MDM.
- Multi-tenant cloud setup (adding a second org after first-boot).
- Zitadel hosting, scaling, or HA configuration.

---

## What changes

### 1. `config/src/model.rs` — two new `AuthConfig` variants

```rust
pub enum AuthConfig {
    DevNull,                 // unchanged — standalone only
    StaticToken { tokens },  // unchanged
    SetupRequired,           // new — default for cloud + edge
    Zitadel {                // new — production provider (wired in stage 2)
        issuer: String,
        jwks_url: String,
        audience: String,
        tenant_id: Option<String>,  // Some = edge (single-tenant)
                                    // None = cloud (multi-tenant)
    },
}
```

`AuthOverlay` gains matching `zitadel` variant. `resolve()` changes the
default logic:

| Role | No `auth:` block in any config layer | Explicit `auth: { provider: dev_null }` |
|---|---|---|
| `standalone` | `DevNull` (unchanged — keeps the "just run it" experience) | `DevNull` |
| `edge` | `SetupRequired` | `DevNull` (intentional override) |
| `cloud` | `SetupRequired` | `DevNull` (intentional override; boot guard still fires in release) |

### 2. `apps/agent/src/main.rs` — boot guard

```rust
// In run_daemon(), before starting anything:
if cfg.role == Role::Cloud
    && matches!(cfg.auth, AuthConfig::DevNull)
    && !cfg!(debug_assertions)
{
    anyhow::bail!(
        "role=cloud with auth=dev_null is not allowed in release builds. \
         Set `auth: {{ provider: zitadel, ... }}` or configure setup mode."
    );
}
```

`SetupRequired` is allowed to boot (it restricts the REST surface itself).
`DevNull` on cloud in release is a hard startup failure.

### 3. `domain-auth` — new `sys.auth.setup` node kind

New manifest `manifests/setup.yaml`:

```yaml
id: sys.auth.setup
display_name: Auth Setup
facets: [isSystem]
containment:
  must_live_under: []
  may_contain: []
  cardinality_per_parent: one_per_parent
slots:
  - name: status
    role: status
    writable: false
    value_schema:
      type: string
      enum: [unconfigured, local, cloud_enrolled]
  - name: mode
    role: status
    writable: false
    value_schema:
      type: string
      enum: [standalone, edge, cloud]
  - name: cloud_url
    role: config
    writable: false
    nullable: true
    value_schema:
      type: string
  - name: enrolled_at
    role: status
    writable: false
    nullable: true
    value_schema:
      type: string
      format: date-time
```

Seeded at boot (in `main.rs` alongside `seed_fleet_node`) when
`cfg.auth == SetupRequired`. The node lives at `/agent/setup`.

> `writable: false` here means **not writable via tenant-facing flow/REST
> paths** — i.e. nobody can `PATCH` these slots through the normal slot
> API. The bootstrapper writes to them internally via the same
> `GraphStore` handle that seeds the node; `writable: false` is a guard
> on the tenant surface, not a universal immutability claim. `enrolled_at`
> is written by the enroll handler on successful cloud handshake; all
> other slots are written by the setup handler.

### 4. `transport-rest` — setup endpoints

New file `crates/transport-rest/src/auth_setup.rs`. The existing
`AppState` gains a `setup_mode: bool` field (or checked via
`AuthConfig::is_setup_required()`). While in setup mode the router
mounts a restricted surface:

#### Transport security requirement

The setup call carries a plaintext password in the cloud body. It MUST
travel over an authenticated transport. Concretely, the agent refuses to
mount `POST /api/v1/auth/setup` on a non-loopback bind unless TLS is
terminated by the agent itself or by a reverse proxy under operator
control. In Phase A the simplest shipping configuration is:

- **Cloud**: bind the REST listener to `127.0.0.1` during first boot and
  require operators to run setup via an SSH tunnel or loopback tooling,
  OR require `tls:` config (cert + key) before the setup route will
  mount. If neither is present the agent logs a hard warning and refuses
  to serve setup on an externally-routable bind.
- **Edge**: same rule; edge first-boot is almost always LAN-local and
  loopback is typically sufficient.

`admin_password` is **never persisted in cleartext**. The cloud setup
handler hashes it with Argon2id (operator-tunable params, default
`m=64MiB,t=3,p=1`) before writing the `sys.auth.user` node. The raw
password leaves memory as soon as the hash is computed.

#### Single-flight guard on setup

The first-boot check is a check-then-act sequence: read `status`, reject
if not `unconfigured`, then mutate. Two concurrent requests would both
pass the check and both swap the provider, silently discarding the
first-returned token and locking the operator out.

The setup handler MUST serialize on an in-process mutex (or a
`tokio::sync::OnceCell`-style single-shot) that covers the entire
check-generate-write-swap sequence. The second caller observes
`unconfigured` → `local` transitioning under the lock and returns
`409 Conflict { "error": "already_configured" }` — not a second token.

**`POST /api/v1/auth/setup`** — first-boot only; rejected if already
configured.

Cloud request body:
```json
{
  "mode": "cloud",
  "org_name": "Acme Corp",
  "admin_email": "admin@example.com",
  "admin_password": "..."
}
```

Edge request body:
```json
{
  "mode": "edge"
}
```

Response (both):
```json
{
  "status": "ok",
  "token": "<initial-admin-bearer-token>",
  "advice": "Store this token — it will not be shown again."
}
```

Effect:
- **Cloud**: writes a `sys.auth.tenant` + `sys.auth.user` node for the
  first org; writes an `AuthConfig::StaticToken` entry (with a strong
  random token) to the YAML config file path; hot-swaps the live
  `Arc<dyn AuthProvider>` on `AppState`; writes
  `/agent/setup.status = "local"` (will become `"cloud_enrolled"` once
  the Zitadel provider lands). Returns the token.
- **Edge**: same as cloud path but `mode = "edge"`; the token scope is
  restricted to edge-local operations.

**`POST /api/v1/auth/enroll`** — connect an already-setup edge to a cloud.
Only valid on `role=edge` where `status == "local"`.

Request:
```json
{
  "cloud_url": "https://cloud.example.com",
  "enrollment_token": "<operator-generated-token>"
}
```

Response:
```json
{
  "status": "enrolled",
  "tenant_id": "org_01ABC",
  "agent_id": "edge-42"
}
```

Effect: exchanges the enrollment token with the cloud's
`POST /api/v1/agents/enroll` endpoint (separate, planned scope); on
success writes `AuthConfig::Zitadel { ... }` to config; hot-swaps
provider; writes `/agent/setup.status = "cloud_enrolled"`. This endpoint
is additive — the cloud enroll side is a follow-on to this scope.

#### Routing matrix by setup state

The three states are mutually exclusive and determine the full REST
surface:

| `status`          | Reachable endpoints                        | All others |
|-------------------|--------------------------------------------|-----------|
| `unconfigured`    | `POST /api/v1/auth/setup` only             | `503 not_configured` |
| `local`           | full REST surface + `POST /api/v1/auth/enroll` (edge only) | — |
| `cloud_enrolled`  | full REST surface; enroll is `409 already_enrolled` | — |

`enroll` is **not** reachable in `unconfigured`; an unconfigured edge
must run `setup` first (which writes `status=local`), then `enroll`.

**All other endpoints** while `status == "unconfigured"` return:
```json
HTTP 503
{ "error": "not_configured", "message": "Run POST /api/v1/auth/setup first." }
```

### 5. `AppState` hot-swap

`AppState.auth_provider` changes from `Arc<dyn AuthProvider>` (immutable
after construction) to:

```rust
Arc<ArcSwap<Arc<dyn AuthProvider>>>
```

`ArcSwap<T>` requires `T: Sized`, so the bare trait object (`dyn
AuthProvider`, unsized) cannot be the payload — `ArcSwap` stores the
`Arc` *around* the trait object. Readers do
`state.auth_provider.load_full()` to get an `Arc<Arc<dyn AuthProvider>>`
and deref through. `arc-swap` is already in the workspace dep set.

### 6. Config write-back

After setup completes, the initial `StaticToken` config is written to the
config file path. If no `--config` flag was given, the agent writes to
`<db-dir>/agent.yaml` by default (parallel to `agent.db`). On next boot
the file layer provides the token without going through setup again.

**agent.yaml is the canonical store of the setup token.** The token
printed in the setup response is a convenience — if the operator loses
it, the same token can be recovered by reading the `static_token` entry
in `agent.yaml` on the agent host. The advice string in the response is
therefore "store this token" (for convenience), not "this token will be
irretrievable" — because it is not.

The file is written with mode `0600` (owner read/write only) and the
containing directory with `0700`. Operators running the agent as a
dedicated service user (recommended) get OS-level protection for free;
the writeback path fails loudly if it cannot set those permissions (for
example, on a world-writable `/tmp`-style directory).

---

## Data flow: cloud first boot

```
Operator  →  POST /api/v1/auth/setup { mode: cloud, org: ..., email: ..., pw: ... }
                │
                ▼
          auth_setup::handle_setup()
                │
                ├─ generate random token (32 bytes, base64url)
                ├─ write sys.auth.tenant + sys.auth.user nodes  (graph)
                ├─ write StaticTokenEntry to config file        (disk)
                ├─ hot-swap AppState.auth_provider              (memory)
                ├─ write /agent/setup slots: status=local       (graph)
                └─ return { token }
```

## Data flow: edge first boot → local account → cloud enroll

```
Operator  →  POST /api/v1/auth/setup { mode: edge }
                │
                ├─ generate random token
                ├─ write config file + hot-swap provider
                ├─ write /agent/setup: status=local, mode=edge
                └─ return { token }

(Later)
Operator  →  POST /api/v1/auth/enroll { cloud_url, enrollment_token }
                │
                ├─ POST <cloud_url>/api/v1/agents/enroll { enrollment_token, agent_id }
                │    └─ cloud returns { tenant_id, zitadel_issuer, audience, service_cred }
                ├─ persist AuthConfig::Zitadel to config file
                ├─ hot-swap provider to ZitadelProvider (stage 2 only)
                ├─ write /agent/setup: status=cloud_enrolled, cloud_url
                └─ return { status: enrolled, tenant_id, agent_id }
```

---

## What is NOT changed by this scope

- `standalone` role: boots with `DevNull` as before; `POST /api/v1/auth/setup`
  is unavailable — the route is not mounted on standalone, so requests
  return `404 Not Found` (not `405`, which would imply a method mismatch
  on a present route).
- Existing `StaticToken` config-file users: the overlay gains a `zitadel`
  tag but `static_token` is unchanged. Old config files continue to work.
- The `DevNullProvider` and `StaticTokenProvider` in `crates/auth`: no
  changes to those impls.
- Fleet transport, engine, blocks-host: no changes.

---

## Phases

**Phase A — this scope (for approval)**

| Item | Crate |
|---|---|
| `AuthConfig::SetupRequired` + `AuthConfig::Zitadel` variants | `config` |
| `AuthOverlay::Zitadel` + `ZitadelAuthOverlay` overlay structs | `config` |
| Role-aware defaults in `resolve()` | `config` |
| Boot guard: `cloud + DevNull + release → bail` | `apps/agent` |
| `sys.auth.setup` node kind + manifest | `domain-auth` |
| Seed `/agent/setup` node at boot when `SetupRequired` | `apps/agent` |
| `POST /api/v1/auth/setup` (cloud + edge) | `transport-rest` |
| 503 gate for all other endpoints in setup mode | `transport-rest` |
| `AppState` hot-swap via `ArcSwap` | `transport-rest` |
| Config write-back after setup | `apps/agent` |
| `POST /api/v1/auth/enroll` (edge→cloud handshake) | `transport-rest` |
| `agent auth setup` + `agent auth enroll` CLI commands | `transport-cli` |
| Client methods: `client.auth.setup()` + `client.auth.enroll()` | `agent-client-rs`, `agent-client-ts` |

**Phase B — Zitadel provider (separate scope)**

- `crates/auth-zitadel/`: `ZitadelProvider` — JWKS fetch, disk cache,
  offline JWT verification, deny-list consumption.
- `POST /api/v1/agents/enroll` on the cloud side (creates a Zitadel service
  account for the enrolling edge).
- Hot-swap from `StaticToken` → `ZitadelProvider` after enroll completes.
- `AuthConfig::Zitadel` wired into `run_daemon()` (Phase A leaves a
  `todo!()` at that branch since `ZitadelProvider` doesn't exist yet).
  Phase A gates the unreachable arm with `#[allow(dead_code)]` on the
  variant's associated fields (or a `zitadel` cargo feature gated off by
  default) to keep `cargo clippy -- -D warnings` green while the provider
  crate is absent. The allow-lint / feature gate is removed in Phase B
  when the arm becomes live.

---

## Acceptance criteria (Phase A)

1. `agent run --role cloud` with no `auth:` config block starts in setup
   mode. `GET /api/v1/nodes` returns `503`. `POST /api/v1/auth/setup` with
   valid body returns `200 { token }` and subsequent `GET /api/v1/nodes`
   with `Authorization: Bearer <token>` returns `200`.

2. `agent run --role cloud --release-profile` with `auth: { provider:
   dev_null }` explicitly set in config boots normally (intentional
   override for CI). Without the explicit override it refuses to start.
   *(Tested via `--cfg debug_assertions` control in test harness.)*

3. `agent run --role edge` with no `auth:` block: same 503 gate; setup
   endpoint works; `/agent/setup` node has `status=local, mode=edge`.

4. `agent run --role standalone` with no `auth:` block: boots with
   `DevNull` as before; no change to existing behaviour.

5. On re-boot after setup, the YAML config file is read, the token is found
   in the `static_token` table, and the agent starts with that provider
   without going through setup again.

6. `agent auth setup` CLI command works end-to-end (calls
   `POST /api/v1/auth/setup`, prints the token).

7. All existing `cargo test -p config`, `cargo test -p graph`, and
   `cargo test -p transport-rest` suites pass without modification.
