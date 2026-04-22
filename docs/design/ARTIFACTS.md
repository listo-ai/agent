# Artifact Storage & Distribution

How large, versioned, signed artefacts move between cloud and edge — block bundles, firmware, snapshot backups, templates, and (later) user documents.

This is the **bulk-bytes** layer. It is deliberately separate from [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md) (which carries control messages) and from [BACKUP.md](BACKUP.md) (which defines *what* a snapshot/template bundle is). Fleet transport announces that an artefact exists; this doc defines where the bytes live, how they get there, and how to fetch them.

Read first: [FLEET-TRANSPORT.md § "Bulk transfer"](FLEET-TRANSPORT.md), [BACKUP.md](BACKUP.md), [AUTH.md](AUTH.md), [OVERVIEW.md](OVERVIEW.md).

---

## 1 — The one rule

**Control plane carries metadata; data plane carries bytes. They never mix.**

- **Control plane** = Zenoh (fleet transport). Small JSON messages: "artefact X version Y is at URL Z, hash H, signed by K". Reliable, ordered, auth-gated.
- **Data plane** = S3-compatible object storage + HTTPS. Large blobs (KB to GB): block bundles, snapshots, templates, docs. Resumable, presignable, cached at the edge.

Why: pushing multi-GB block bundles through a pub/sub fabric abuses the protocol, blows out every backend's message-size limit, and loses resumable-range semantics. Every mature artefact-distribution system (Docker, GitHub Releases, Kubernetes, every IoT OTA vendor) converged on this split. We don't re-invent it.

Corollary: **the fleet transport never carries a payload larger than `smallest_edge_ram / 10`.** If it would, it's a URL message instead. Enforced at the `FleetTransport` trait boundary ([FLEET-TRANSPORT.md § "Failure modes"](FLEET-TRANSPORT.md) — `PayloadTooLarge`).

---

## 2 — Optional at build time

The artefact subsystem is **compiled out by default**. A standalone single-device deployment (no cloud, no fleet) doesn't need object-store I/O, presigned URLs, or a cloud bucket — and shouldn't pay for them in binary size, dependency surface, or attack surface.

Same pattern as [FLEET-TRANSPORT.md § "Backend selection"](FLEET-TRANSPORT.md): trait in `spi`, one crate per backend, Cargo features gate compilation, config picks at runtime.

| Cargo feature | Pulls in | When to enable |
|---|---|---|
| *(none)* | `NullArtifactStore` from `spi`; every method returns `Disabled`. | Standalone single-device; no cloud; demos; CI fixtures that don't exercise artefact flows. |
| `artifacts-s3` | `data-artifacts-s3` (object_store crate with S3 backend). | Any deployment that talks to Garage, R2, AWS S3, Backblaze B2, or any S3-compatible store. **The common case.** |
| `artifacts-local` | `data-artifacts-local` (object_store local-filesystem backend). | Dev loops; single-node deployments where the "cloud" is a directory; air-gapped sites. |
| `artifacts-azure` / `artifacts-gcs` | object_store Azure/GCS backends. | Customers who require those clouds. Ship only when asked for. |

Features are additive. A cloud-tier build enables `artifacts-s3`; a dev build enables `artifacts-local`; a true standalone build enables neither and the subsystem simply isn't there.

Runtime config decides which compiled-in backend to use:

```yaml
# config.yaml — cloud tier
artifacts:
  backend: s3
  endpoint: https://garage.listo.internal
  region: zone-eu-1
  bucket_template: "listo-{tenant}"
  credentials_from: env  # or kms, or static (dev only)
```

```yaml
# config.yaml — standalone
# (no artifacts: section; NullArtifactStore is active)
```

`artifacts: null` or the section absent = disabled at runtime even if a backend is compiled in. Same knob shape as `fleet: null`. One consistent story for optional subsystems.

### 2.1 — Capability advertisement

The subsystem is always visible in the capability map — even when disabled — so clients can distinguish "this agent can't do artefacts" from "artefacts are temporarily unavailable":

| Runtime state | Capability advertised |
|---|---|
| Backend compiled in and configured (e.g. `artifacts: { backend: s3, … }`) | `artifacts.s3.v1` |
| Backend compiled in but `artifacts: null` | `artifacts.null.v1` |
| No backend feature compiled in | `artifacts.null.v1` |

In all three cases the `ArtifactStore` trait is present on `AppState`; a `NullArtifactStore` is active in the latter two and every call returns `ArtifactError::Disabled`. The capability string tells a Studio client whether to hide artefact UIs (`null`) or show them (`s3`, `local`, etc.). Never: a missing capability key.

---

## 3 — Storage layout

One bucket per tenant. Prefix structure mirrors the domain model.

```
s3://listo-<tenant>/
  blocks/<block_id>/<version>/bundle.tar.zst      # pluggable blocks (cloud → edge)
  blocks/<block_id>/<version>/manifest.json       # signed manifest, small
  snapshots/<agent_id>/<ts>.listo-snapshot        # DR backups (edge → cloud)
  templates/<template_id>/<version>.listo-template # portable configs (both directions)
  firmware/<channel>/<version>/listod.listo       # OTA bundles (cloud → edge)
  docs/<user_id>/<doc_id>/<rev>                   # user docs (later stage)
```

Properties:

- **Tenant isolation.** Zitadel's `org_id` claim ([AUTH.md](AUTH.md)) maps to a bucket or a prefix depending on backend — see § 3.1 below. In both modes, cross-tenant access is structural: either the caller's credentials don't include the bucket, or IAM conditions refuse the prefix.
- **Path shape is inspectable.** A tenant-scoped key list answers "what backups exist for this device" with no metadata lookup.
- **Immutable history where it matters.** `blocks/<id>/<version>/` is write-once — no overwrites, new versions get new keys. Enables cache-forever semantics on the edge.
- **Retention is app-owned, not purely S3 lifecycle.** See § 10. Lifecycle rules are the belt; app-driven deletion tied to the Postgres receipts table is the braces. Running just one of them creates drift (orphan objects or receipts that 404).

### 3.1 — Bucket-per-tenant vs prefix-per-tenant

**Not every backend can afford a bucket per tenant.** AWS S3 caps accounts at 100 buckets (soft-raisable to 1000 — still insufficient at multi-thousand tenants). Garage and R2 have no such ceiling. The layout adapts:

| Backend | Mode | Tenant maps to | Enforcement |
|---|---|---|---|
| **Garage** | bucket-per-tenant | `listo-<tenant>` | Access key scoped to bucket. |
| **Cloudflare R2** | bucket-per-tenant | `listo-<tenant>` | API token scoped to bucket. |
| **AWS S3** | prefix-per-tenant | `listo-shared/<tenant>/…` within one bucket | IAM policy with `s3:prefix` / `Resource` conditions per tenant, issued as per-tenant short-lived credentials via STS. |
| **Backblaze B2** | bucket-per-tenant | `listo-<tenant>` | Application-key scoped to bucket. |

The `ArtifactKey` shape (`blocks/<id>/<version>/…`, `snapshots/<agent>/<ts>.listo-snapshot`, …) is identical in both modes — what changes is what sits *in front* of the key:

- Bucket mode: `s3://listo-<tenant>/<key>`.
- Prefix mode: `s3://listo-shared/<tenant>/<key>`.

The `bucket_for(tenant)` + `prefix_for(tenant)` helpers in `spi::artifacts` return the right pair per backend configuration. Callers never hand-format. The presigner emits scoped credentials or policy per mode; the cross-tenant and cross-agent checks in § 8.2 remain identical because they operate on parsed key components, not raw paths.

---

## 4 — The SPI trait

Lives in [`crates/spi/src/artifacts.rs`](../../crates/spi/src/artifacts.rs). Object-safe so `AppState` can hold `Arc<dyn ArtifactStore>`.

```rust
/// Opaque handle — an `object_store::Path` under the hood, but callers
/// treat it as a string key. Backends normalise internally.
pub type ArtifactKey = String;

/// Content hash asserted by the publisher and verified by the consumer.
/// Not trusted blindly — the signature in the control message covers it.
pub struct Integrity {
    pub sha256: [u8; 32],
    pub size: u64,
}

#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Stream bytes into storage. Multipart-aware; backends handle chunking.
    async fn put(&self, key: &ArtifactKey, bytes: ByteStream) -> Result<(), ArtifactError>;

    /// Stream bytes out. Caller verifies integrity against the expected hash.
    async fn get(&self, key: &ArtifactKey) -> Result<ByteStream, ArtifactError>;

    /// Cheap existence check — HEAD request, no body.
    async fn head(&self, key: &ArtifactKey) -> Result<Option<Integrity>, ArtifactError>;

    /// Mint a time-limited URL the caller can hand to another party.
    /// Direction = Put (upload) or Get (download). TTL is advisory; backends
    /// clamp to their own max.
    async fn presign(
        &self,
        key: &ArtifactKey,
        direction: PresignDirection,
        ttl: Duration,
    ) -> Result<PresignedUrl, ArtifactError>;

    /// Stable backend id — surfaces in capabilities as `artifacts.<id>.v1`.
    fn id(&self) -> &'static str;
}
```

A zero-config `NullArtifactStore` ships in `spi`. Every method returns `ArtifactError::Disabled`. `AppState` holds one by default; `artifacts: null` is the absence configuration.

Key points:

1. **`ArtifactKey` is not opaque.** Callers construct keys from typed components (`snapshots::key(tenant, agent, ts)`) to keep the layout rules in one crate. No stringly-typed fan-out.
2. **`presign` is first-class.** Presigned URLs are the primary mechanism; direct `put`/`get` exist for server-side operations (cloud agent moving bytes between prefixes, garbage collection).
3. **Integrity is in the SPI, not the backend.** Backends store bytes. Hash verification happens in `domain-artifacts` against values carried in the control message.

---

## 5 — Backend selection

One crate per backend. Same shape as `transport-fleet-*`.

| Crate | Cargo feature | Provides | Status | Positioning |
|---|---|---|---|---|
| [`data-artifacts-s3`](../../crates/data-artifacts-s3/) | `artifacts-s3` | `artifacts.s3.v1` | 🔜 planned | **Primary.** Uses `object_store` S3 backend. Target: Garage (self-hosted), Cloudflare R2 (managed, no egress), AWS S3 (enterprise), Backblaze B2. |
| `data-artifacts-local` | `artifacts-local` | `artifacts.local.v1` | 🔜 planned | Dev loops, single-node deployments, air-gapped sites. Filesystem-backed via `object_store::local`. |
| `data-artifacts-azure` / `-gcs` | `artifacts-azure` / `artifacts-gcs` | `artifacts.azure.v1` / `artifacts.gcs.v1` | ⏳ future | Added when a customer asks. Implementation is ~50 lines each via `object_store`. |

### 5.1 — Reference backend: Garage

The self-hosted default is [Garage](https://garagehq.deuxfleurs.fr/) — pure-Rust, S3-compatible, geo-distributed, single-binary. Rationale:

- **Rust-native.** Same ecosystem discipline as picking Zenoh over NATS — stay where we can read the source.
- **Multi-site by design.** Replication zones are first-class, which matches "cloud + many edges + regional residency". Makes "customer data must stay in the EU" a config change, not a re-architecture.
- **Minimal ops.** Single binary, no ZK/etcd, no operator CRDs. Listo is a platform, not a storage company.
- **AGPL + co-op governed.** No acquisition rug-risk.

### 5.2 — Managed fallbacks

Same `artifacts-s3` binary speaks to any S3-compatible endpoint. The code doesn't change; the config does.

| Target | Why |
|---|---|
| **Cloudflare R2** | No egress fees — decisive when edges pull multi-GB bundles. Managed cloud tier for Listo-hosted deployments. |
| **AWS S3** | Enterprise customers already on AWS. Egress cost is theirs. |
| **Backblaze B2** | Cheapest per-GB; modest egress. For cost-sensitive self-managed deployments. |

### 5.3 — S3 compatibility surface

We restrict ourselves to the subset of S3 that every credible backend implements:

- PUT (single + multipart), GET (with ranges), HEAD, DELETE, LIST
- Presign (PUT + GET)
- Object Lock **with a bucket-level default retention period** configured at bucket creation — not caller-specified. Per-object Object Lock is theatre if a compromised uploader can PUT objects with zero retention. Backups go into buckets with default `compliance`-mode retention matching the [BACKUP.md § 7](BACKUP.md) policy (e.g. 30d hot + 90d cold).
- Lifecycle rules, including **`AbortIncompleteMultipartUpload` with a 7-day threshold** — aborted multipart uploads from flaky edges accumulate and are billed silently otherwise. Non-negotiable.

Explicitly out of scope: SSE-KMS, Intelligent-Tiering, S3 Inventory, S3 Select, Replication (we handle multi-region at the deployment layer, not through the backend).

A CI smoke test runs the full `data-artifacts-s3` integration suite against a real Garage instance and a real R2 bucket — asserting every operation above works on both. Passing both is the contract.

---

## 6 — Flow A: cloud → edge (block install / OTA / template push)

```
Publisher          Cloud Agent          Zenoh             Edge Agent              S3 / Garage
    │                   │                 │                    │                       │
    │─presign-upload───►│                 │                    │                       │
    │◄─{url, expires}───│                 │                    │                       │
    │─PUT bundle────────────────────────────────────────────────────────────────────── ►│
    │─presign-upload───►│ (for manifest)  │                    │                       │
    │─PUT manifest────────────────────────────────────────────────────────────────────► │
    │                   │                 │                    │                       │
    │                   │──publish────────►                    │                       │
    │                   │  fleet.<t>.<a>.cmd.block.install     │                       │
    │                   │  { id, version, key, sha256, sig,    │                       │
    │                   │    size_hint, content_retained_until}►                       │
    │                   │                 │                    │                       │
    │                   │  (edge may be offline — message holds, URL is NOT in it)     │
    │                   │                 │                    │                       │
    │                   │◄──presign-download──────────────────│                       │
    │                   │     (JWT, key, size_hint)           │                       │
    │                   │──{url, expires}─────────────────────►│                       │
    │                   │                 │                    │──GET presigned URL───►│
    │                   │                 │                    │◄──bytes──────────────│
    │                   │                 │                    │                       │
    │                   │                 │                    │ verify sig + hash     │
    │                   │                 │                    │ install block         │
    │                   │                 │                    │                       │
    │                   │◄─block.installed│◄───────────────────│                       │
```

**The control message carries identity, not a URL.** Presign TTL is a data-plane concern scoped to one fetch; artefact lifetime (months) is orthogonal. Conflating them — putting a 5-minute URL in a durable fleet command — means any edge offline for more than a few minutes gets a stale URL and has to round-trip anyway. Keep the two lifetimes separate:

1. **Publisher uploads via the presigner, not directly.** CI, Studio, and CLI publishers hit `POST /api/v1/artifacts/presign-upload` on the cloud agent, JWT-authenticated with a *publisher* scope (see § 8.4). They PUT bundle and `manifest.json` to the returned URLs. The cloud agent is the **single choke-point for both edge and publisher writes** — no parallel credential path.
2. Cloud agent publishes `fleet.<tenant>.<agent>.cmd.block.install` with `{ id, version, key, sha256, signature, content_retained_until }`. No URL. `content_retained_until` is the artefact's lifecycle-policy expiry — months, not minutes — so the edge can still decide *whether* to fetch after a long offline window without needing the cloud for the decision.
3. Edge receives the command (Zenoh holds it across offline windows). At fetch time, edge calls `POST /api/v1/artifacts/presign-download` with the key from the command, JWT-authenticated with the edge's own identity. Cloud returns a fresh URL with TTL sized for the expected fetch (see § 6.1).
4. Edge fetches via HTTPS. `object_store` handles resume/retry for transport-level failures.
5. Edge verifies the ed25519 signature against the trust root for this artefact kind (§ 8.5) **and** the hash. Any mismatch = hard reject, no retry, no install. The signature covers the hash, so a post-transport hash mismatch is semantically a signature failure; treat them identically.
6. Edge installs and publishes `fleet.<tenant>.<agent>.event.block.installed`.

The edge is **outbound-only** throughout. No inbound ports.

### 6.1 — TTL sizing for presigned URLs

TTL is a single-fetch concern. The presigner sizes it from the edge's size hint:

```
ttl = clamp(
    expected_size_bytes / assumed_min_bandwidth * safety_factor,
    min_ttl,
    backend_max_ttl,
)
```

Defaults: `min_ttl = 5 min`, `assumed_min_bandwidth = 256 kbit/s`, `safety_factor = 2`. A 4 GB firmware bundle over a slow link gets ~9 h; a 50 KB template gets `min_ttl`. The edge passes `expected_size_hint` in the `presign-download` request (carried in the control message's `size_hint` field, which the publisher knew at upload time).

---

## 7 — Flow B: edge → cloud (snapshot backup upload)

```
Edge Agent          Zenoh          Cloud Agent           S3 / Garage        Postgres
    │                 │                  │                      │                  │
    │─backup.request─►│─────────────────►│                      │                  │
    │  {size, kind}   │                  │ check quota/auth     │                  │
    │                 │                  │ mint presigned PUT   │                  │
    │◄───reply────────│◄─────────────────│                      │                  │
    │  {url, expires} │                  │                      │                  │
    │                 │                  │                      │                  │
    │─PUT multipart──────────────────────┼─────────────────────►│                  │
    │◄──── 200 ──────────────────────────┼──────────────────────│                  │
    │                 │                  │                      │                  │
    │─backup.uploaded►│─────────────────►│ record receipt ──────┼──INSERT─────────►│
    │  {url, sha256,  │                  │                      │                  │
    │   sig, manifest}│                  │                      │                  │
    │◄───ack──────────│◄─────────────────│                      │                  │
```

1. Edge asks cloud for a presigned upload URL via `POST /api/v1/artifacts/presign-upload`. Request carries the intended size, bundle kind (`snapshot` / `template`), and the edge's JWT.
2. Cloud validates JWT → tenant, checks quota, and **verifies the key's agent component matches `jwt.agent_id`** (see § 8.2) — a compromised edge for agent A cannot presign writes under `snapshots/<agent_B>/…`. Then mints a presigned PUT URL scoped to the single key.
3. Edge uploads via HTTPS multipart directly to the bucket. Cloud agent is not in the byte path.
4. Edge publishes a completion event keyed by `sha256` — the natural idempotency key. If the edge crashes between PUT-success and ack-received, the retry hits the same receipt row (`INSERT ... ON CONFLICT (sha256) DO NOTHING`); no duplicates.
5. Cloud records the receipt in Postgres — `(tenant, agent, ts, key, sha256, signature, manifest, content_retained_until)` — for listing, retention, and restore UIs. See § 10 for retention ownership and orphan reconciliation.

**Same path for publishers.** CI, Studio, and CLI publishers (Flow A, step 1) use the exact same `presign-upload` endpoint — the only difference is a publisher-scoped JWT that permits writes under `blocks/…`, `templates/…`, or `firmware/…` prefixes. One choke-point, one audit surface, no parallel credentials.

Why the receipt lives in Postgres, not the bucket: listing by agent + time range is a SQL query in milliseconds; listing an S3 prefix is an API call and eventually paginates. Cloud UX needs the SQL path; restoration needs the bucket path; both are cheap.

---

## 8 — Auth, isolation & signing

### 8.1 — Layered enforcement

Three layers, all mandatory:

| Layer | Enforced by |
|---|---|
| **Control-plane auth** | Zenoh access control on `fleet.<tenant>.<agent>.*` ([FLEET-TRANSPORT.md § "Auth"](FLEET-TRANSPORT.md)). The edge can't even request an upload URL for a different tenant. |
| **Presigner scope check** | JWT claims (`org_id`, `agent_id`, `scope`) must match the requested artefact key's components. See § 8.2. |
| **Data-plane auth** | Presigned URL scope. The URL embeds the exact key and expires. Even if leaked, it grants access to one object for one TTL window. |

### 8.2 — Presigner scope rules

The cloud agent's presigner is the single choke-point. For every request:

```
1. jwt.org_id                   must equal   tenant(key)
2. jwt.agent_id (if edge scope) must equal   agent(key) for edge-scoped keys
3. jwt.scope                    must permit  operation × prefix(key)
```

Rule 2 is the **intra-tenant cross-agent** check — it's not enough that the JWT is for the right org. A compromised edge authenticated as `agent_A` within `org_X` must not be able to presign reads or writes under `snapshots/<agent_B>/…`. The presigner refuses with `403` regardless of whether the org matches.

Tested by two assertions, both in CI:

- JWT for org A presenting key for org B → `403`.
- JWT for `(org_X, agent_A)` presenting key `snapshots/<agent_B>/…` → `403`.

### 8.3 — Publisher scopes

Publishers (CI, Studio, CLI) are not edges — they have no `agent_id` claim. The `scope` claim distinguishes what they can write:

| Scope claim | Can presign writes under | Notes |
|---|---|---|
| `agent` (implicit, when `agent_id` present) | `snapshots/<jwt.agent_id>/…`, `templates/<…>` (reads + writes own templates) | Default for edges. |
| `publisher:blocks` | `blocks/<block_id>/…` (only for block IDs owned by the publisher) | CI systems building blocks. |
| `publisher:templates` | `templates/<template_id>/…` | Studio / CLI pushing templates into a tenant. |
| `publisher:firmware` | `firmware/<channel>/…` | Platform firmware publishers. Highest trust; typically Zitadel-gated to a small group. |
| `admin` | `**` within the tenant | Operator tooling, break-glass. Audited. |

Ownership of a `block_id` or `template_id` is tracked in Postgres (tenant + asset registry), not in the JWT — keeps the token small and lets ownership rotate without token re-issue.

### 8.4 — Secret handling in artefacts

Secrets inside artefacts (`Secret`-portability slots in templates; sealed sections in snapshots) are **age-encrypted before upload** per [BACKUP.md § 5.2](BACKUP.md). The object store sees ciphertext. A bucket compromise doesn't leak credentials.

### 8.5 — Signing keys and trust roots

Ed25519 signatures on artefact manifests are load-bearing for every security claim in this doc. What an edge trusts as a signing root depends on the artefact kind. **Three kinds, three roots, three key-rotation stories:**

| Artefact kind | Trust root | Lives on | Rotation |
|---|---|---|---|
| **Firmware / OTA** (`firmware/…`) | Platform signing key. Rooted at Listo (the vendor). | Pinned into the edge at image build time. Rotation ships a new image that trusts both old + new key, then a later image drops the old. | Slow (quarterly or on compromise). |
| **Templates & snapshots** (`templates/…`, `snapshots/…`) | Tenant signing key. Rooted at the tenant's Zitadel org. | Published under `sys.auth.tenant/<tenant>/keys` — a graph node with `config.signing_keys` (plural, for rotation overlap). Edge fetches the tenant's public keys on claim and caches them. | Medium (monthly; rotates via graph node update). |
| **Blocks** (`blocks/…`) | Block-publisher signing key. Rooted at the publisher's identity in the block registry. | Fetched alongside the block manifest via the registry. The tenant's policy (`sys.blocks.policy/trusted_publishers`) lists which publisher keys are acceptable per block namespace. | Publisher-controlled; the tenant's policy gates which keys count. |

The three roots never cross: a firmware image cannot be signed by a tenant key, and a block cannot be signed by the platform key. The manifest carries `trust_root: "platform" | "tenant" | "block_publisher"` and the verifier picks the right root accordingly. Mismatch (`trust_root: "platform"` but subject is `blocks/…`) = hard reject.

**Revocation** is list-based:

- Platform keys: revocation list baked into subsequent firmware images; edges also check a signed revocation list fetched on each reconnect.
- Tenant keys: rotation-by-replacement (remove the key from the graph node), propagated via the same event stream that carries every other slot change.
- Block-publisher keys: publisher-controlled, tenant-gated. A tenant removes a compromised publisher from `trusted_publishers`.

**Edge-side verification** is one codepath: `verify(manifest) → Result<VerifiedArtifact, VerificationError>`. It picks the trust root by manifest kind, looks up the key ring, and verifies. No per-kind branching in callers.

This is a lot, and most of it isn't implemented yet. A companion doc `SIGNING.md` will land alongside the first non-scaffolding PR — for now, this section fixes the contract so it isn't hand-waved.

---

## 9 — Where the code lives

Follows the standard layering ([CODE-LAYOUT.md](../../../SKILLS/CODE-LAYOUT.md), [HOW-TO-ADD-CODE.md](HOW-TO-ADD-CODE.md) Q4).

```
contracts/spi/src/artifacts.rs            # ArtifactStore trait, NullArtifactStore,
                                          #   ArtifactError, ArtifactKey helpers.
                                          #   Key-construction helpers (snapshots::key,
                                          #   blocks::key) — one source of truth for
                                          #   the layout in §3.

agent/crates/domain-artifacts/            # Pure logic: decide-to-fetch, verify sig+hash,
                                          #   local-cache policy, GC of stale artefacts.
                                          #   No HTTP, no S3 SDK. Consumes ArtifactStore.

agent/crates/data-artifacts-s3/           # Feature: "artifacts-s3". object_store-backed
                                          #   ArtifactStore. Presigning, multipart,
                                          #   retry. Compiled out by default.

agent/crates/data-artifacts-local/        # Feature: "artifacts-local". Filesystem
                                          #   ArtifactStore for dev / air-gapped.

agent/crates/transport-rest/src/
  artifacts.rs                            # Thin handlers: presign-upload,
                                          #   presign-download, receipt endpoints.
                                          #   Each <20 lines (Rule I).

agent/crates/transport-cli/               # `agent artifacts {list,fetch,put}` —
                                          #   thin clap wrapper over agent-client-rs.

agent-client-{rs,ts,dart}/                # Client surfaces mirror REST.
```

No artefact logic lives in `transport-fleet-zenoh`. Fleet carries the control message and stops — handler deserialises, calls `domain-artifacts`, returns. Same layering discipline as every other surface.

`data-backup` ([BACKUP.md § 6](BACKUP.md)) produces the bundle bytes; `data-artifacts-*` distributes them. Two jobs, two crates.

---

## 10 — Retention, reconciliation & orphans

Retention is **app-owned**. The Postgres receipts table is the source of truth for "what exists"; S3 lifecycle rules are a backstop for objects that slip through cracks. Running only one of them creates drift:

- **Lifecycle-only.** S3 deletes objects after 30 days; Postgres rows live on; restore UI shows entries that 404.
- **App-only.** Application deletes Postgres rows; object storage bill grows forever with orphaned bytes.

Neither is acceptable. The rule:

### 10.1 — The retention job

A scheduled job in the cloud agent owns retention end-to-end. Runs hourly.

```
for each (tenant, prefix) in retention_policies:
    for each receipt older than policy.retention_duration:
        DELETE FROM receipts WHERE (tenant, key) = (...)
        ArtifactStore::delete(key)
    if both succeed: commit
    if either fails: log + alert; leave for next run (idempotent)
```

The retention policy lives on a `sys.artifacts.retention` node per tenant, with slots per prefix (`snapshots.retention_days`, `templates.retention_days`, etc.) — same "observable state is a node" discipline as the rest of the platform.

### 10.2 — S3 lifecycle as belt-and-braces

Configure S3 lifecycle to delete `snapshots/*` at `retention_days + 30` — a grace period that kicks in only if the app-owned job is broken for a month. Lifecycle is **never the primary deletion mechanism**; it exists to bound the blast radius of a stuck retention job.

### 10.3 — Orphan reconciliation

A weekly job walks the bucket and the receipts table and flags divergence:

- Object exists, no receipt → orphan. Caused by: cloud agent crashing between S3 PUT succeeding and receipt INSERT. Action: delete object (using `sha256` from object metadata to confirm no in-flight upload is targeting it), or INSERT a synthetic receipt if the manifest parses cleanly.
- Receipt exists, no object → phantom receipt. Caused by: lifecycle deletion without app-side cleanup (bug), or manual bucket ops. Action: delete the receipt, log, alert.

Divergence counts are exported as metrics (`artifacts_orphans_total`, `artifacts_phantoms_total`). A healthy deployment reports zero. Non-zero is a bug to investigate, not a normal operating condition.

### 10.4 — Content-retained-until in control messages

The `content_retained_until` field on `cmd.block.install` (§ 6) is computed from `receipt.created_at + policy.retention_duration`. Edges that see a command whose `content_retained_until` is already past can skip fetching — the object's been reaped. This is a rare but important case: an edge offline for longer than the retention window wakes up, sees a stale command, and doesn't waste cycles on a guaranteed-404 presign.

---

## 11 — Local cache on the edge

Edges cache fetched artefacts in `var/cache/artifacts/` keyed by `sha256`. Purpose:

- **Survive re-installs.** Reinstalling the same block version after a supervisor restart shouldn't re-download.
- **Enable atomic swap.** Fetch to cache, verify, then rename into the blocks directory. Failed downloads never touch the live tree.
- **Support offline retry.** A block install scheduled during a network outage has the bytes already if they've been pre-fetched.

Cache eviction is **pinned + LRU**, not plain LRU. A naive LRU gets wrecked by a one-off large fetch (firmware, big snapshot) evicting the hot working set of installed blocks.

- **Pinned entries** — any `sha256` referenced by a currently-installed block or active subscription is pinned and never evicted while referenced. The reference set is the graph: `blocks-host` reports which `sha256`s back live blocks, and the cache consults it.
- **LRU over the rest** — unpinned entries (pre-fetched but not yet installed, leftovers from uninstalled blocks) evict in LRU order until the cap is met.
- **Cap with headroom** — `artifacts.cache_bytes` (default 2 GB) is the unpinned budget; pinned bytes are separate. Running out of pinned room is an operational alert, not a silent eviction.

Cache entries are immutable — keyed by content hash, so there's no staleness concept.

---

## 12 — Failure modes

| Failure | Observable | Recovery |
|---|---|---|
| Presigned URL expired before edge fetches | `ArtifactError::Expired`; edge knows from `expires_at` in the presign response. | Edge requests a fresh URL via `presign-download` — cheap round-trip, no control-plane re-publish needed. This is *normal* for long-offline edges; not an error condition. |
| Hash mismatch on download | `ArtifactError::IntegrityFailure`. **Hard reject, no retry.** The signature covers the hash; mismatch means tampered bytes or storage corruption, not transport noise (which HTTPS already handles). Retrying costs GB of egress for zero chance of success. | Publish `block.install.failed` with reason; operator investigates. Treat identically to signature-invalid. |
| Signature invalid | Hard reject, log with key-id; no retry. | Operator verifies signing-key rotation hasn't desynced (see § 8.5); re-publishes with correct signature or rotates the trust-root node. |
| Object store unreachable from edge | Transient: exponential backoff retry in `data-artifacts-s3`. Persistent: surface on `sys.agent.artifacts` node `status.reachable`. | Network recovery resumes fetches; no state lost (cache is content-addressed). |
| Cloud agent crashes between S3 PUT and receipt INSERT (Flow B) | Object exists, no receipt. Edge retries `backup.uploaded` keyed by `sha256` → `INSERT … ON CONFLICT DO NOTHING` completes on second attempt. | Edge's retry path handles the common case. For lost edges (never retry), the § 10.3 reconciliation sweep catches the orphan. |
| Duplicate completion event | Edge crash-and-retry after successful upload → two `backup.uploaded` messages. | `sha256` is the idempotency key on the receipts table. Second INSERT is a no-op. No duplicate rows. |
| Tenant bucket missing | Cloud agent provisioning bug. `NotFound` on first upload attempt. | Alert; provisioner re-runs on tenant-create event. |
| Orphaned objects (receipts deleted but bucket entry remains, or vice versa) | Reconciliation job's `artifacts_orphans_total` / `artifacts_phantoms_total` metrics go non-zero. | Investigate why the retention job or app-deletion path failed; fix root cause. Reconciliation is a safety net, not a normal operating mode. |
| Cache disk full | Eviction runs per § 11 policy (pinned entries preserved); if still full, `ArtifactError::CacheFull`. | Raise `artifacts.cache_bytes` or reduce retention in `sys.artifacts.config`. Alert fires before hard-full. |

---

## 13 — Not in v1

| Not in v1 | Why |
|---|---|
| Peer-to-peer edge ↔ edge fetch | Same reasoning as fleet-transport P2P — NAT-traversal complexity for a rare need. If a regional cache helps, run a Garage node in the region. |
| Client-side encryption above age-sealed secrets | Covered by bucket-level encryption at rest + age for secrets. Adding a per-tenant KMS envelope is additive when a regulated customer asks. |
| Differential / delta artefacts | Full bundles only. Dedup at storage layer (rustic repo as a future `data-artifacts-rustic` backend) is how this gets solved, not per-artefact delta formats. |
| Public / anonymous artefacts | Every URL is presigned, short-lived, and tenant-scoped. No public sharing of artefacts through this subsystem — if we ever need public block listings, they're a separate service. |
| Google Drive / Dropbox sync | External sync adapter, future work. When it lands, it's a block — consuming whatever surfaces `listo-blocks-sdk` exposes (per Rule C; blocks never path-dep `spi` directly). The sentence about "consumes `ArtifactStore`" in an earlier draft was wrong: if blocks ever need artefact access, the SDK gets a curated wrapper, not a raw trait handle. |

---

## 14 — Scaffolding status

As of this writing, the crates and modules named in § 9 exist as
skeletons — file trees, Cargo manifests, trait signatures, doc
comments — with no logic yet. Method bodies are `todo!()`; Cargo
features are declared; READMEs state the intent and the gating rule.
Nothing is registered in the workspace `Cargo.toml` yet — wiring
into the build lands with the first implementation PR.

Checklist of what exists:

- [x] `contracts/spi/src/artifacts.rs` — `ArtifactStore` trait,
      `NullArtifactStore`, error / presign / integrity types, typed
      key constructors.
- [x] `agent/crates/domain-artifacts/` — `Cargo.toml`, `src/lib.rs`,
      module stubs (`verify`, `cache`, `distribute`, `keys`), README.
- [x] `agent/crates/data-artifacts-s3/` — `Cargo.toml` with
      `object_store` dep, `src/lib.rs` with `S3ArtifactStore` impl
      skeleton, README.
- [x] `agent/crates/data-artifacts-local/` — `Cargo.toml`,
      `src/lib.rs` with `LocalArtifactStore` impl skeleton, README.
- [x] `agent/crates/transport-rest/src/artifacts.rs` — route
      inventory + handler signatures (no routing registered).
- [ ] Workspace `Cargo.toml` registration (deferred to impl PR).
- [ ] `agent-client-{rs,ts,dart}` surfaces (deferred to impl PR).
- [ ] `transport-cli` `agent artifacts` subcommand (deferred).
- [ ] CI smoke test against real Garage + R2 (deferred).
- [ ] `SIGNING.md` companion doc (deferred; interim contract in § 8.5).
- [ ] Retention job + orphan reconciliation implementation (deferred; contract in § 10).

### 14.1 — Review feedback applied

The design in this doc reflects a round of review that surfaced real
flaws in the first draft. Changes since first draft:

- **Decoupled presign TTL from control-message lifetime** (§ 6). URL is
  minted at fetch time via `presign-download`, not embedded in the
  fleet command. Long-offline edges no longer receive stale URLs.
- **Publisher uploads go through the presigner** (§ 6, § 7). One
  choke-point for edge + publisher writes; no parallel credential path.
- **Signing trust roots made explicit** (§ 8.5). Platform / tenant /
  block-publisher keys each have their own root, rotation story, and
  revocation mechanism.
- **Intra-tenant cross-agent check** (§ 8.2). Presigner asserts
  `jwt.agent_id == agent(key)`, not just `jwt.org_id == tenant(key)`.
- **Bucket-per-tenant vs prefix-per-tenant per backend** (§ 3.1). AWS
  S3 can't scale to bucket-per-tenant; prefix mode with IAM conditions
  is the documented fallback.
- **App-owned retention + orphan reconciliation** (§ 10). Postgres
  receipts + S3 lifecycle no longer drift silently.
- **Idempotency on completion events** (§ 7, § 12). `sha256` is the
  natural key; retries are safe.
- **Hash mismatch is a hard reject** (§ 12). Signature covers the
  hash; mismatch is semantically a signature failure, not transport
  noise. No retry.
- **Multipart-abort lifecycle rule + bucket-level default retention**
  (§ 5.3). Closes the two sneakiest operational gaps.
- **Pinned + LRU cache, not plain LRU** (§ 11). One-off large fetches
  can't evict the working set.
- **Capability advertisement for the null backend** (§ 2.1). Clients
  distinguish "can't" from "temporarily unavailable".

## 15 — Summary

- **Control plane (Zenoh) carries metadata; data plane (S3) carries bytes.** Never mix.
- **Optional at build time.** Cargo features gate each backend; `NullArtifactStore` is the absence.
- **Garage is the self-hosted reference; any S3-compatible endpoint works.** Swapping backends is config, not code.
- **One bucket per tenant, prefix-structured by domain.** Tenant isolation is structural.
- **Presigned URLs + edge-side verify.** The cloud agent is never in the byte path.
- **`data-backup` makes bundles; `data-artifacts-*` distributes them.** Two jobs, two crates.
