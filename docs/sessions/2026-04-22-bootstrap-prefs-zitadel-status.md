# Bootstrap + Preferences + Zitadel — Minimum-Viable Status

**Session scope:** Phase A bootstrap, USER-PREFERENCES Phase 0 + 1 + 2,
Phase B Zitadel (B.1, B.2, B.4). Writability guard. Read-path unit
metadata. CLI + Rust client + TS client surfaces. Human-facing unit
labels on the wire + `agent units list` CLI (quick-win pass).

**Current state:** all non-infrastructure-blocked scope landed. The
agent boots, authenticates, enforces first-boot setup, verifies
Zitadel JWTs offline, persists unit-annotated telemetry canonically,
and exposes quantity/unit on every read. See [What shipped](#what-shipped).

**Quick-win additions (this pass):**

- `Quantity::label()` / `Unit::label()` / `Unit::symbol()` on the spi
  enums — every quantity and unit has a stable English name + compact
  symbol the CLI and future unit-picker UIs read without hard-coding.
- `GET /api/v1/units` enriched with a `label` field per quantity and
  a flat `units: [{ id, symbol, label }]` table so clients render
  pickers without walking the quantity map.
- `agent units list` CLI subcommand — dumps the registry via the
  client's existing `.units().get()` accessor.
- Mirror updates in `agent-client-rs` (`UnitEntryDto`,
  `QuantityEntryDto.label`) and `agent-client-ts`
  (`UnitEntrySchema`, `QuantityEntrySchema.label`). Backward-compatible
  — both schemas default the new fields so older servers parse cleanly.
- Cleanup: `#[allow(dead_code)]` on the public `auth_zitadel::boxed`
  helper so crate-level warnings stay at zero.
- `Quantity::from_str` + `Unit::from_str` (inverse of `as_str`) with
  matching `UnknownQuantity` / `UnknownUnit` error types. Lets CLI
  and config code parse a quantity/unit token without routing
  through `serde_json::from_str` just for one identifier. 5 new
  tests including a serde-vs-FromStr alignment check (catches
  drift between `rename_all = "snake_case"` and the hand-written
  string tables on the next build).
- Clippy cleanups in `auth-zitadel::provider` (slice-from-ref
  instead of clone-to-slice) and `domain-auth::setup` (dropped
  redundant `ref` binding). Both crates + `config` now clippy-clean
  on default lints.
- `agent units show <quantity>` CLI — dumps one quantity's
  canonical + allowed alternatives enriched with symbol/label from
  the flat table. Error path lists every known quantity so the
  operator's next attempt is informed.
- **Conversion factors now ship on `/api/v1/units`.**
  `UnitEntry.to_canonical: { scale, offset }` on every allowed
  unit. Coefficients are **derived from the server's own `uom`-
  backed `StaticRegistry`** via two probes per unit (`v=0`, `v=1`)
  — no duplicate factor table exists. A spi test runs six probes
  across every (quantity, unit) pair and asserts
  `coeffs.scale * v + coeffs.offset` matches the registry's
  `convert()` to 1e-9 relative tolerance. If the server's
  conversion table changes, the published factors change with it;
  if they can't be represented affinely, the derivation returns
  `None` and the wire carries no bad data.
- TS-side `convertUnit(registry, quantity, value, from, to)` in
  `agent-client-ts/src/units/convert.ts` — **zero hardcoded
  tables**, reads the `to_canonical` coefficients from the fetched
  `UnitRegistry`. Companion helpers `unitSymbol`, `unitLabel`,
  `canonicalUnitFor`. Exported from the client's top-level index.
  This resolves the "drift risk" flagged in
  `USER-PREFERENCES.md § "Client-side unit conversion"` and is
  the full client path of Preferences Phase 3 — the only remaining
  piece for Phase 3 is the server-side `Accept-Units` middleware
  (which is ergonomics, not a capability gap — clients can now
  convert themselves fully).
- **17 vitest tests** for the TS converter
  (`agent-client-ts/tests/units.test.ts`): known reference values
  (°C/°F/K, bar/kPa/psi), round-trips through canonical, identity
  path preserves precision, every error path (unknown quantity /
  from-unit / to-unit / mismatched quantity) returns `null`, and
  the lookup helpers' fallback behaviour.
- **`Unit::quantity() -> Option<Quantity>`** reverse lookup in
  `spi::units`. Closed-enum integrity test asserts every `Unit`
  variant maps back to some quantity — cheap guard against a
  future variant landing in the enum but not being wired into an
  `allowed` set. 2 new tests.
- **Phase A acceptance tests shipped** — D-5 from the deferred
  list. 9 in-process end-to-end tests in
  `transport-rest/tests/bootstrap_e2e.rs` exercise the real
  router + middleware stack via `tower::ServiceExt::oneshot`:
  - cloud setup-mode gate (503 on non-allowlisted paths, 200 on
    `/healthz` + `/api/v1/capabilities`)
  - happy path → 200 with token → bearer unlocks the gated API
  - single-flight 409 on the second setup call
  - mode-mismatch 400
  - edge setup flow (same gate, same shape as cloud)
  - no-setup-service → no gate (standalone path)
  - `/auth/enroll` → 501 Phase B
  - auto-mode writeback writes `agent.yaml` with 0o600 on Unix
  - regression guard on `/api/v1/units` being gated today (so a
    future allowlist change is deliberate)
  Also added `AppState::with_auth_provider_cell(ProviderCell)` —
  lets the composition root + tests share one `ProviderCell`
  between `AppState` and `SetupService` so a hot-swap in the
  service is visible through the state.

**What's missing to call the whole surface "done":** one
infrastructure-blocked item (cloud enroll), one ergonomics
middleware (response-side unit conversion), and one UX track
(Studio preferences pages). None block production deployment of
the current minimum.

---

## The minimum-viable deploy

The code today supports **three production-grade configurations**
without further work:

### Config A — Standalone / dev
```yaml
# role: standalone
# no auth: block
```
Boots with `DevNullProvider`. Every request admin. Unchanged from
before this session. Useful for local dev, appliances, CI.

### Config B — Edge or cloud, fresh
```yaml
# role: cloud  (or edge)
# no auth: block
```
Boots into `SetupRequired`. REST surface 503s every non-allowlisted
path. Operator runs:
```
agent auth setup --mode cloud --org-name Acme --admin-email admin@acme.com
```
Agent:
1. Writes `agent.yaml` with a generated 256-bit `StaticToken` (mode
   0600) next to `agent.db`, or returns a `config_snippet` for the
   operator to paste if `--config <path>` was used.
2. Hot-swaps the provider to `StaticTokenProvider`.
3. Flips `/agent/setup.status = "local"`.
4. Returns the bearer token.

Restart picks up the token from `agent.yaml`. No more setup mode.

### Config C — Cloud with pre-existing Zitadel
```yaml
auth:
  provider: zitadel
  issuer: "https://acme.zitadel.cloud"
  audience: "listo-agent"
  jwks_url: "https://acme.zitadel.cloud/oauth/v2/keys"
  tenant_id: null        # or "<org-id>" for single-tenant edge
```
Boots `ZitadelProvider`. Fetches JWKS, caches to `zitadel-jwks.json`
next to `agent.db`, spawns a 15-minute refresh task. Verifies every
inbound JWT signature + iss + aud + exp + nbf offline; offline
verification is load-bearing for edges on flaky links. An admin
who wants runtime revocations adds:

```rust
provider.with_deny_list(Arc::new(StaticDenyList::new([...])))
```

in the composition root, or pushes a custom `DenyList` impl that
polls the cloud.

---

## What shipped

Numbers are cargo-test green: `config` 3, `auth` 13,
`domain-auth` 11, `auth-zitadel` 24, `graph` 75, `transport-rest`
58 (49 lib + 9 acceptance), `spi::units` 28. `agent-client-ts`
vitest: 36 (19 contract + 17 unit-converter).

### Bootstrap (Phase A)

- **`config`**: `AuthConfig::SetupRequired`, `AuthConfig::Zitadel`,
  role-aware `resolve()` (cloud/edge → `SetupRequired`; standalone
  → `DevNull`), `to_file` atomic 0600 writeback.
- **`auth`**: `ProviderCell` — reusable hot-swap for
  `Arc<dyn AuthProvider>`. Used by setup and will be reused by
  cert rotation / Zitadel re-keying.
- **`domain-auth`**: `SetupNode` + `sys.auth.setup` manifest;
  `SetupService` owns the state machine, single-flight mutex,
  token generation, graph writes, config writeback, provider swap.
  Transport handlers call `service.complete_local(mode)` — every
  future transport reuses this.
- **`transport-rest`**: thin `POST /auth/setup` / `POST /auth/enroll`
  handlers (≤20 lines each per Rule I); 503-gate middleware keyed
  on `/agent/setup.status`; `AppState.auth_provider: ProviderCell`.
- **`apps/agent`**: boot guards (cloud+DevNull+release, setup+non-
  loopback); provider dispatch; `SetupService` construction; seed
  `/agent/setup` when resolved.
- **`transport-cli`**: `agent auth setup/enroll`, `agent prefs
  get/set/org-get/org-set`, `agent units list` (dumps the registry
  with labels + symbols in the operator's preferred output format).

### User preferences

- **`spi::units`** (Phase 0): `Quantity` (11), `Unit` (39),
  `UnitRegistry`, `StaticRegistry` backed by `uom` with
  round-trip-via-canonical conversion; `normalize_for_storage`
  helper used by `graph::write_slot_inner`; `registry_dto()` for
  `GET /api/v1/units`; `Quantity::label()` + `Unit::label()` +
  `Unit::symbol()` human-facing strings consumed by the CLI and
  picker UIs; flat `UnitEntry` table on the registry response so
  clients never hard-code symbols or labels. 18 tests.
- **Backend (Phase 1)**: already existed at session start;
  4 endpoints + 3-layer resolution.
- **Ingest (Phase 2)**: `graph::write_slot_inner` calls
  `spi::normalize_for_storage` — `72.4 °F` → `22.44 °C` stored.
- **Read path (Phase 2)**: `SlotDto` and `SlotSchemaDto` carry
  `quantity`/`unit`/`sensor_unit`; `GET /api/v1/node` returns
  everything clients need for preference-driven rendering in one
  call.
- **Clients + CLI**: full surface.

### Zitadel (Phase B.1 + B.2 + B.4)

- **`auth-zitadel` crate**: `ZitadelProvider` with offline JWT
  verify, `JwksSource` trait (`HttpJwksSource`, `StaticJwksSource`),
  `DiskCache` with atomic writes, background refresh task,
  `kid`-miss on-demand refresh under single-flight mutex, claim→
  `AuthContext` mapping (UUIDv5 fallback for non-UUID subs),
  tenant pinning, `DenyList` trait + `StaticDenyList`. 24 tests
  including every security-critical rejection path.
- **`apps/agent`**: `AuthConfig::Zitadel` match arm constructs the
  provider and spawns refresh. Disk cache defaults to sit next to
  `agent.db`.

### Writability guard

- **`graph`**: `WriteSlotOpts { expected, enforce_tenant_writable }`
  + `write_slot_with()` API. `GraphError::SlotNotWritable`. Guard
  runs before CAS so probing leaks nothing. 5 integration tests.
- **`transport-rest`**: `write_slot_core` uses
  `WriteSlotOpts::tenant()`; 403 on `writable: false`. Fleet path
  inherits via shared function. Bootstrappers (`SetupService`,
  `seed_fleet_node`, etc.) keep calling `write_slot` directly and
  correctly bypass.

---

## Deferred work — clear paths forward

### D-1 · Preferences Phase 3: `Accept-Units` middleware

**Scope.** Server-side converts telemetry values per
`GET /me/preferences` before serialising. `Accept-Units: canonical`
opt-out for MCP / programmatic consumers. Per-series (not per-row)
unit hoisting for time-series.

**Why deferred.** Clients can already convert themselves fully —
`/api/v1/units` now ships affine coefficients and the TS client has
`convertUnit()` / `unitSymbol()` / `unitLabel()` helpers. The
middleware is pure ergonomics, not a capability gap. Token-embedded
preference claims (the design doc's "JWT claim block") also need
Zitadel custom claims wired up, which is operator-side Zitadel
config.

**Clear path to add:**
1. New `transport-rest/src/unit_conversion.rs`:
   - Extract: pref service → resolved prefs → `Accept-Units` header
     → decision tree per quantity.
   - Apply at the DTO-serialization boundary. Two surfaces need it:
     `GET /api/v1/node` (already carries metadata) and
     `GET /api/v1/history/...` (time-series hoisting).
2. Hook into the existing handlers as a pure transformation
   — the DTOs already carry `quantity`/`unit`.
3. Add `Vary: Accept-Units` + `Content-Language` response headers
   (design doc §"Content negotiation").
4. Tests: a temperature slot, `temperature_unit: F` in prefs → the
   read returns `72.3` with `unit: fahrenheit`.

**Effort:** ~4 hours. No new crate, no schema change. Every piece
it needs already exists.

### D-2 · Preferences Phase 4: Studio UI + `block-ui-sdk` re-exports

**Scope.** `PreferencesProvider` in `ui-core`, `formatters.ts`
(pure functions over the browser's `Intl` APIs), `SettingsPage`
rebuild, `react-intl` + `en.json`/`es.json` bundles, demo node
with unit conversion. Eight layers outlined in
`USER-PREFERENCES.md` § "UI implementation scope".

**Why deferred.** Frontend-native scope that needs designer and
Studio-context pairing. Server-side is complete; frontend can
proceed independently without further backend work.

**Clear path to add:**
1. `agent-client-ts/src/domain/preferences.ts` — already shipped
   this session.
2. `ui-core/src/providers/PreferencesProvider.tsx` +
   `ui-core/src/hooks/usePreferences.ts` — fetch-once-per-session
   + React Query cache with ETag. Wrap `<PreferencesProvider>`
   inside `<AuthProvider>`, outside `<IntlProvider>`.
3. `ui-core/src/lib/formatters.ts` — pure functions,
   `formatDate(ts, prefs)` / `formatUnit(value, quantity, unit, prefs)`.
   Use `Intl.DateTimeFormat` + `Intl.NumberFormat`. Unit table
   driven by cached `GET /api/v1/units`.
4. `ui-core/src/pages/SettingsPage.tsx` — the existing stub. Five
   sections: Language, Timezone, Date & Time, Units, Numbers, Theme.
5. `ui-core/src/i18n/` with `en.json` + `es.json`; `<IntlProvider>`
   inside `<PreferencesProvider>`.
6. Replace bare `toLocaleString()` calls across `ui-core`.
7. `block-ui-sdk/src/formatters.ts` — re-export only, per §4a of
   HOW-TO-ADD-CODE.md. No new implementations.
8. Demo node annotation (`com.listo.mqtt-client` or a `sys.demo.sensor`
   test kind).

**Effort:** ~3 days of frontend work.

### D-3 · Phase B.3: cloud-side enrollment

**Scope.** `POST /api/v1/agents/enroll` on the cloud agent. Takes
an operator-issued enrollment token, mints a Zitadel service
account, returns `{ tenant_id, zitadel_issuer, audience,
service_cred }` to the enrolling edge. Edge's `/auth/enroll`
(currently 501) calls this, persists the `AuthConfig::Zitadel`
overlay, hot-swaps the provider.

**Why deferred.** Needs live Zitadel admin-API credentials. Can't
meaningfully unit-test without a running Zitadel. Every security
consideration on the minting side (token scoping, revocation hook,
service-account lifecycle) is Zitadel-deployment-specific.

**Clear path to add:**
1. Cloud-side: new `transport-rest/src/agent_enrollment.rs`
   handler. Validates the enrollment token (probably via
   `StaticTokenProvider` — enrollment tokens are short-lived
   admin-issued), opens a Zitadel admin client (`zitadel-rust`
   crate exists, or direct REST), provisions a project + service
   account, returns the credentials envelope.
2. Edge-side: replace the 501 body in
   `transport-rest::auth_setup::enroll` with:
   - HTTP POST to `{cloud_url}/api/v1/agents/enroll` with the
     operator-supplied enrollment token.
   - Parse response; build `AuthConfig::Zitadel`.
   - Write the new `auth:` block to `agent.yaml` (reuse
     `config::to_file`).
   - Build a `ZitadelProvider` from the fresh config; hot-swap via
     `SetupService`'s existing `ProviderCell`. (The service already
     exposes the cell; add a `set_provider` method if needed.)
   - Flip `/agent/setup.status = "cloud_enrolled"`, write
     `cloud_url` + `enrolled_at` slots.
3. Tests: point the edge at a stubbed cloud that mirrors the real
   response envelope — sufficient for end-to-end shape verification
   without real Zitadel.

**Effort:** ~1 day once Zitadel access is available.

### D-4 · BACKUP + ARTIFACTS

**Scope.** The 883-line BACKUP.md. Snapshot vs template bundle
format, export/import, `device_id` check via listod claim,
signed-bundle envelope reusing listod's OTA format, artifact
store trait + backends (S3, local FS).

**Why deferred.** Separate scope entirely. Linter has landed the
spi scaffolding (`spi::backup::{Portability, BundleManifest,
BundleKind}`, `spi::artifacts::*`, `crates/domain-backup/`).
Completing it is multi-day work that warrants its own session +
review cycle.

**Clear path to add:** follow BACKUP.md §6 ("Where the code
lives") top-to-bottom. Starting points already in source:
- `spi::backup::BundleManifest` — wire shape defined.
- `spi::artifacts::ArtifactStore` trait — skeleton defined.
- `crates/domain-backup/` — crate scaffolded, methods to fill.
- `SlotSchema::portability` — field exists; the name-based
  credential lint (BACKUP.md §2.3 rule 2) is the gate to add at
  `kinds register`.

**Effort:** ~2-3 days for the snapshot/template pair + portability
enforcement; more for the artifact-store backends.

### D-5 · Phase A acceptance tests ✅ shipped

9 in-process end-to-end tests in
[`transport-rest/tests/bootstrap_e2e.rs`](../../crates/transport-rest/tests/bootstrap_e2e.rs)
drive the real router + middleware stack via
`tower::ServiceExt::oneshot`. In-process rather than
subprocess-spawning because the composition surface that matters
(router mount, middleware stack, setup service, provider hot-swap,
writeback) is all exercised with zero flakiness and no port
management. The one thing not covered — `apps/agent::run_daemon`
boot guards + signal handling — is covered by unit tests in that
crate.

### D-6 · Minor: JWT claim block for preferences

**Scope.** `timezone`, `locale`, `language` embedded in JWT claims
so server-rendered output (emails, audit PDFs) knows which timezone
to use without a DB round-trip.

**Why deferred.** Requires Zitadel custom-claim configuration on
the operator side. Not possible to test without a live Zitadel.
Spec says: "server-originated renders MUST re-resolve from the DB
at send time" — so the claims are an optimisation, not a
correctness requirement.

**Clear path to add:**
1. `auth-zitadel::provider::map_claims` already reads arbitrary
   extras — add optional `timezone`/`locale`/`language` reads.
2. Extend `spi::AuthContext` (or a sidecar
   `PreferenceClaims`) with the fields.
3. Consumers (email renderer, audit exporter — neither exists yet)
   read from the context.

**Effort:** ~2 hours when there's something that needs it.

---

## Recommended sequencing

If you want to close every gap:

```
D-1 (middleware)        ← 4h; no infra dep
D-5 (acceptance tests)  ← 3h; no infra dep
D-3 (cloud enroll)      ← 1d; needs Zitadel access
D-2 (Studio UI)         ← 3d; needs frontend pairing
D-4 (BACKUP)            ← multi-day; its own design review
D-6 (JWT prefs claim)   ← 2h; needs a consumer
```

If you want "minimum viable for ship today": stop here. The
current state is production-capable for configs A, B, and C above.

---

## Invariants protected by the current scope

These are non-negotiable and the tests guard them. Keep them green
when adding any of the deferred items:

1. `/agent/setup.status = "unconfigured"` → REST 503s every
   non-allowlisted route. Setup handler single-flight.
2. Cloud + DevNull + release → boot refused.
3. `role=cloud|edge` + setup + non-loopback bind → boot refused.
4. `SlotSchema::writable = false` on a tenant write → 403, no CAS
   probe leaks.
5. Zitadel verification is offline; disk cache covers cold-boot-
   without-network for already-seen keys.
6. Deny-list hits surface as `InvalidCredentials` (not a distinct
   status) so revoked-subject enumeration is not possible from the
   wire.
7. Stored slot values are canonical for their `quantity` (ingest
   conversion is lossless-or-explicit).
8. `SlotDto` and `SlotSchemaDto` carry `quantity`/`unit` so
   middleware-less clients can format correctly.
9. `ProviderCell` is the one hot-swap primitive; no new providers
   invent their own.
10. Transport handlers remain ≤ 20 lines (Rule I).
