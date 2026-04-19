# Auth Seam — Implementation Scope

Scope for the first landing of the auth system: **the shape, not the IdP**. Ship the plumbing — `AuthContext` threaded through every handler, audit events carrying `actor` + `tenant`, subject namespaces parameterised by tenant — before fleet transport lands, so neither fleet nor plugin-contributed handlers have to retrofit auth cross-cuttingly later.

Authoritative references: [AUTH.md](../design/AUTH.md) for the long-term Zitadel-based identity model, [FLEET-TRANSPORT.md](../design/FLEET-TRANSPORT.md) § "Auth" for why the seam has to exist first, [VERSIONING.md](../design/VERSIONING.md) for capability gating.

**Core rule for this landing:** every mutating code path consults an `AuthContext`, no code path hardcodes identity, and the default provider is a marked-as-dev `DevNullProvider` — never the fallback for prod. When we plug Zitadel in later it swaps one crate, nothing above the trait changes.

---

## Scope rails

**In:**

| In |
|---|
| `AuthContext { actor, tenant, scopes }` type in `crates/spi` |
| `AuthProvider` trait in `crates/spi` — one async fn: `resolve(headers) → AuthContext` |
| `crates/auth` crate provides two concrete impls: `DevNullProvider` (default today) and `StaticTokenProvider` (shared bearer for localdev-two-user scenarios) |
| Axum extractor `FromRequestParts for AuthContext` — every handler that mutates data must declare it as an extractor arg |
| Every existing `POST`/`PATCH`/`DELETE` route in `transport-rest` takes the extractor and threads `AuthContext` into the store call |
| Audit events (`graph_events` table today, audit stream later) grow `actor: NodeId` + `tenant: String` columns; migrations included |
| `sys.auth.user` + `sys.auth.tenant` node kinds registered in `domain-extensions` (or a new `domain-auth` crate) — mirrors Zitadel entities as graph nodes per [EVERYTHING-AS-NODE.md](../design/EVERYTHING-AS-NODE.md) |
| Scope enum: `ReadNodes`, `WriteNodes`, `WriteSlots`, `WriteConfig`, `ManagePlugins`, `ManageFleet`, `Admin` — minimal, expandable |
| Scope check helper: `ctx.require(Scope::WriteSlots)?` returning a structured `AuthError` that serialises to 403 |
| `AuthContext` flows into `GraphStore` mutations — `create_child_as(ctx, ...)`, `write_slot_as(ctx, ...)`, etc. — not a global |
| Dev-null provider logs `"⚠️ auth is dev-null — all requests pass as tenant=default, actor=local"` at startup and every 15 minutes |
| Tests: every mutation handler covered by a "no auth context" negative test and a "wrong scope" negative test |

**Out (explicitly deferred):**

| Out | Why / when |
|---|---|
| Zitadel integration (JWKS fetch, JWT verify, OIDC flows) | Separate landing once the seam is proven. Plug new `ZitadelProvider` into the trait. |
| Refresh-token plumbing, PKCE, device-flow | Same — downstream of Zitadel integration. |
| MFA, SSO, social login | Zitadel gives these for free when plugged in; no work here. |
| Edge agent service accounts | Needs fleet transport first (edges need cloud to authenticate to); this landing is local REST only. |
| Per-row / per-node RBAC policies (beyond tenant isolation) | Stage after. The scopes enum is coarse-grained on purpose for v1. |
| Studio login UI | Today's `AgentClient.connect({skipCapabilityCheck: true})` continues; the UI "sign in" button is a Stage-N addition. |
| Cross-tenant sharing, delegation, impersonation | Explicit non-goals for v1. |
| API key / service-account management UI | Later — needs a `/settings/users` surface that doesn't exist yet. |
| Rate limiting, abuse controls | Separate concern; orthogonal. |

## The types

Lives in [`crates/spi/src/auth.rs`](../../crates/spi/src/auth.rs). Small, cheap to clone, serializable for test fixtures.

```rust
/// Who is making this request, what tenant they act on, what they're
/// allowed to do. Stamped on every inbound request before handlers run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Stable identifier — usually a `NodeId` pointing at an
    /// `sys.auth.user` or `sys.auth.service-account` node. For the
    /// dev-null provider: the special id `NodeId::nil()` with actor
    /// string `"local"`.
    pub actor: Actor,

    /// The tenant this request operates against. Enforced in
    /// `GraphStore` mutations — attempting to mutate a different
    /// tenant's subtree fails with `AuthError::WrongTenant`.
    pub tenant: TenantId,

    /// Coarse-grained permission set. Handlers call
    /// `ctx.require(Scope::WriteSlots)?` before mutating.
    pub scopes: ScopeSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Actor {
    /// A human user backed by an identity-provider session.
    User   { id: NodeId, display_name: String },
    /// A machine identity — service account, extension publisher, edge agent.
    Machine { id: NodeId, label: String },
    /// Dev-null default. NEVER present in production — startup refuses
    /// to boot with a `DevNullProvider` when the deployment role is
    /// `cloud` and the build is `--release`.
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    ReadNodes,
    WriteNodes,
    WriteSlots,
    WriteConfig,
    ManagePlugins,
    ManageFleet,
    /// Implies all others. Reserved for initial bootstrap + emergency.
    Admin,
}

/// Compact set backed by a bitflag — scope membership check is O(1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeSet(u32);

impl AuthContext {
    pub fn require(&self, scope: Scope) -> Result<(), AuthError> { ... }
    pub fn owns(&self, tenant: &TenantId) -> bool { ... }
}
```

### The provider trait

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Resolve an `AuthContext` from raw request metadata. Called once
    /// per request by the axum extractor. Never blocks on network —
    /// Zitadel's verifier uses a cached JWKS refreshed on a timer.
    async fn resolve(&self, headers: &HeaderMap) -> Result<AuthContext, AuthError>;

    /// Identifies the provider in logs + capability manifest. Present
    /// so a deployment can surface "auth.zitadel.v1" or
    /// "auth.dev_null.v1" in `GET /api/v1/capabilities`.
    fn id(&self) -> &'static str;
}
```

Providers live behind Cargo features in a new `crates/auth` crate — same pattern planned for fleet transport, same reason: swappable at build-time, selected at config-time.

| Crate feature | Provider | Enabled by default |
|---|---|---|
| `auth-dev-null` | `DevNullProvider` — stamps every request as `(Local, default, Admin)` | Yes (dev) |
| `auth-static-token` | `StaticTokenProvider` — reads a table of `(token → AuthContext)` from config | No |
| `auth-zitadel` | (not in this landing) | No |

The `--release` build refuses to start with `DevNullProvider` as the active provider when `role == cloud`. Edge/standalone can run with dev-null; cloud cannot.

## The extractor

Axum extractor in `transport-rest`:

```rust
#[async_trait]
impl<S: AppStateLike> FromRequestParts<S> for AuthContext {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S)
        -> Result<Self, Self::Rejection>
    {
        state.auth_provider().resolve(&parts.headers).await
    }
}
```

Handlers declare it like any other extractor:

```rust
async fn write_slot(
    ctx: AuthContext,
    State(s): State<AppState>,
    Json(req): Json<WriteSlotReq>,
) -> Result<Json<WriteSlotResp>, ApiError> {
    ctx.require(Scope::WriteSlots)?;
    let gen = s.graph
        .write_slot_as(&ctx, &req.path, &req.slot, req.value)
        .map_err(ApiError::from_graph)?;
    Ok(Json(WriteSlotResp { generation: gen }))
}
```

No middleware layer — extractors are stronger: a handler that forgets `AuthContext` is a compile-time signal to reviewers (the route lacks auth), not a silent middleware gap.

## Threading into GraphStore

Today `graph::GraphStore` methods take only content args. They grow `_as` variants that take `&AuthContext`:

| Today | New |
|---|---|
| `create_child(parent, kind, name)` | `create_child_as(ctx, parent, kind, name)` |
| `delete(path)` | `delete_as(ctx, path)` |
| `write_slot(path, slot, value)` | `write_slot_as(ctx, path, slot, value)` |
| `transition(path, new)` | `transition_as(ctx, path, new)` |
| `add_link(source, target)` | `add_link_as(ctx, source, target)` |
| `remove_link(id)` | `remove_link_as(ctx, id)` |

The old signatures stay (internal seed / test helpers need them), but REST handlers switch over to `_as`. Audit events emitted from inside `_as` carry `ctx.actor` and `ctx.tenant`.

**Tenant enforcement in `_as`:** attempting to mutate a path outside `ctx.tenant`'s subtree returns `GraphError::WrongTenant`. Today with only one tenant (`default`) this is a no-op check — but the code path exists so the day multi-tenant cloud lights up, every mutation already validates.

## Audit event migration

| Column | Before | After |
|---|---|---|
| `actor` | absent | non-null, `NodeId` — defaults to "local-dev-null" nil for backfilled events |
| `tenant` | absent | non-null, `String` — defaults to `"default"` for backfilled events |
| `scope_used` | absent | nullable, the specific scope that authorised the mutation (for debugging permission denials later) |

Migration `002_audit_actor_tenant.sql` adds the columns, backfills existing rows with sentinels, and makes them `NOT NULL`. Data-sqlite + data-postgres migrations both ship; [TESTS.md § "Migration round-trip"](../design/TESTS.md) adds a test.

## Auth-as-nodes

Per [EVERYTHING-AS-NODE.md § "The agent itself is a node too"](../design/EVERYTHING-AS-NODE.md), auth entities are first-class graph nodes. Register two kinds:

```yaml
# crates/domain-auth/manifests/user.yaml
kind: sys.auth.user
facets: [isIdentity, isSystem]
containment:
  must_live_under: [{ kind: sys.auth.tenant }]
slots:
  display_name: { role: config,   type: string }
  email:        { role: config,   type: string, nullable: true }
  scopes:       { role: config,   type: array, items: { type: string } }
  last_seen:    { role: status,   type: string, format: date-time, nullable: true }
  enabled:      { role: config,   type: boolean, default: true }
```

```yaml
# crates/domain-auth/manifests/tenant.yaml
kind: sys.auth.tenant
facets: [isIdentity, isSystem, isContainer]
containment:
  must_live_under: []
  may_contain:
    - { kind: sys.auth.user }
    - { kind: sys.auth.service_account }
```

Seed one default tenant + one default user at boot when the graph is empty:

- `/auth/default` (tenant)
- `/auth/default/local-dev` (user, scopes: `Admin`) — the identity `DevNullProvider` stamps requests with.

When Zitadel lands, a sync task mirrors Zitadel users/orgs into these kinds; nothing above the graph changes.

## Staged landing

| Stage | What | Bail-out signal |
|---|---|---|
| **1a — Types + provider trait** | `AuthContext`, `Scope`, `ScopeSet`, `AuthProvider`, `AuthError` in `spi`. New `crates/auth` with `DevNullProvider` + `StaticTokenProvider`. Tests only — no wiring yet. | Scope enum grows past ~10 variants → pause and model as a policy DSL instead of a flat enum. |
| **1b — Wire into REST** | Axum extractor, every mutating route declares `ctx: AuthContext`, `GraphStore::*_as` variants added, old sigs kept for seeds / tests. `AppState` grows `auth_provider: Arc<dyn AuthProvider>`. | `rjsf` property-panel form submit breaks because headers strip in dev → fix extraction to tolerate missing scheme prefix. |
| **1c — Audit fields** | Migration `002_audit_actor_tenant.sql`; `GraphEvent` struct gains `actor`, `tenant`, `scope_used`; every `_as` writes them; existing event log backfilled with sentinels; SSE stream serialises them. | Migration fails on a production-ish SQLite with existing data → drop-and-recreate is acceptable pre-v1; post-v1 this is a hard stop. |
| **1d — Auth-as-nodes + seed** | `domain-auth` crate with `sys.auth.tenant` + `sys.auth.user` kinds. Seed `/auth/default` + `/auth/default/local-dev` on first boot. DevNullProvider's `Actor::Local` resolves its `NodeId` from the seeded user. | Containment rules clash with existing `/agent/` subtree → move auth subtree to `/auth/` (decided up-front; included here as reminder). |
| **1e — Capability manifest** | `host_capabilities()` adds `auth.dev_null.v1` (or whichever provider is active). Plugins that `require: auth.zitadel.v1` refuse to load on a dev-null build. | No issue foreseen — same mechanism as existing capabilities. |

1a–1e target one shipping increment; 1b + 1c are the most invasive (every mutation handler touched).

## Testing

| Test category | What it covers |
|---|---|
| Provider unit tests | Dev-null always resolves. Static-token resolves by token, rejects unknown. |
| Extractor tests | Missing header → 401. Bad header → 401. Good header → context present with right tenant + scopes. |
| Handler negative tests | Every `POST`/`PATCH`/`DELETE` route: missing `AuthContext` → `Rejection`. Wrong scope → 403 with structured error. |
| Cross-tenant | Context for tenant A, request on path in tenant B's subtree → `GraphError::WrongTenant` → 403. |
| Audit backfill | Migration applied to a pre-auth db preserves existing events, new events carry actor+tenant. |
| Capability manifest | `GET /api/v1/capabilities` includes `auth.<provider>.v1`. |
| Dev-null guard | `role=cloud` + `--release` + dev-null provider → startup aborts with clear error. |

## Security non-promises

Things this landing explicitly does NOT assert:

- Dev-null is **not** authentication. Don't expose a dev-null agent on the public internet.
- Static-token is **not** a long-term strategy. It's for two-user local-dev multi-actor scenarios (watching multi-actor audit events render differently).
- Tenant isolation today is trust-based (provider stamps it, store enforces it). Real isolation arrives with real tokens: tenant claim in the JWT, signed by the IdP.
- Scopes are coarse. "Write slots on anything in this tenant" is one scope today; finer RBAC per node kind / per path is a later landing.

These aren't bugs — they're the boundary of what "ship the seam before fleet" means. Everything fits through the same trait, so upgrading is additive.

## What NOT to do in this landing

- **No middleware-only auth.** If the extractor is optional / the middleware is a layer, a handler can forget it and ship insecure. Extractor-per-route makes "this route has no auth" a visible diff.
- **No tenant sniffing from the URL path.** Tenant comes from the token, never from the query string. (Even today with one tenant, write the plumbing this way.)
- **No bypass env var** for testing. Tests use `StaticTokenProvider` with crafted fixtures; production builds never have a "skip auth" switch.
- **No auth checks inside `GraphStore`'s old method names.** The old `write_slot(path, ...)` without `_as` exists for seeds and tests only; REST never calls it after 1b. A clippy lint + code review rule forbids new callers.
- **Don't put the provider into a `thread_local!` or task-local global.** `AuthContext` is an explicit argument; invisible context is how bypasses happen.
- **Don't add "service account" entity support here.** That's an edge-authenticating-to-cloud concern — couples to fleet transport. This landing is REST only.

## Open questions

- **Does `GraphEvent` on the SSE stream expose `actor`/`tenant` to all subscribers, or filter per-tenant before send?** Filter before send, eventually. First landing: expose — single tenant today, no practical leak. Flag with a TODO tied to the multi-tenant-cloud story.
- **Where does the `StaticTokenProvider`'s table live?** `crates/config` overlay: `auth: { static_tokens: { <token> → { actor, tenant, scopes } } }`. Never commit tokens to git; dev-fixture file ships as `.gitignored`.
- **Does the bootstrap "seed default tenant + user" run on every empty boot, or once and then drift is owned by the operator?** Run once when `/auth/` subtree is absent. Subsequent boots respect whatever's there. Matches how `sys.core.station` is seeded.
- **Should `Admin` scope include itself as an audit tag** (so you can tell from audit logs "this was an admin override")? Yes — `scope_used` captures whatever the handler `require`d, which is the narrowest scope used. Admin listed in `scopes` but `scope_used` shows the specific action's scope.
- **How does Studio know whether auth is dev-null and should hide/show the login button?** `GET /api/v1/capabilities` exposes the active provider id. Studio branches on it.

## One-line summary

**Ship the auth shape before fleet — `AuthContext` as an axum extractor on every mutating REST handler, `GraphStore::*_as` variants carrying it into the store, audit events growing `actor` + `tenant` + `scope_used`, auth entities represented as graph nodes under `/auth/`, provider behind a trait with dev-null + static-token impls today — so fleet transport and future Zitadel integration both plug into a plumbed seam instead of retrofitting auth cross-cuttingly.**
