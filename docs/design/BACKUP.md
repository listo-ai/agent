# Backup & Restore

How the agent exports, imports, and replicates state — across a single device's
lifetime (disaster recovery) and across a fleet (template deployment).

This doc exists because "backup" is two different problems sharing one word, and
mixing them leads to bad answers (MAC-binding, global UUIDs, device-specific
secrets leaking into fleet rollouts). The split here is the one industry
converged on in the 2020s: **immutable, signed artifact + declarative
configuration**, separated by lifecycle.

Read first: [EVERYTHING-AS-NODE.md](EVERYTHING-AS-NODE.md),
[STORAGE-TIERS.md](STORAGE-TIERS.md), [VERSIONING.md](VERSIONING.md),
[FLEET-TRANSPORT.md](FLEET-TRANSPORT.md),
[ARTIFACTS.md](ARTIFACTS.md) (where bundles land and travel in cloud
builds — see § 6.2 for the three operating modes and § 6.4 for the
integration seam), and
[listod/SCOPE.md](../../../listod/SCOPE.md) (the signed-bundle format we reuse).

This doc owns: **bundle format** (snapshot vs template, envelope,
manifest, portability rules), **export/import/restore logic**, and
**retention cadence** (when to snap, how often). Distribution — upload
to object storage, presigning, fetch, tenant bucketing, store-level
retention — lives in [ARTIFACTS.md](ARTIFACTS.md). See § 6.4 on how
the two meet.

---

## 1 — The two bundle types

One concept, one word, two shapes. Never collapse them.

| | **Snapshot** | **Template** |
|---|---|---|
| Purpose | Disaster recovery for *this* device | Deploy or share logical configuration across *any* device |
| Scope | Everything needed to resurrect the exact agent state | Logical graph + kind configuration; nothing device-local |
| Contents | SQLite + Postgres dumps, `data/` state dir, slot history, time-series, audit log, claim tokens (sealed), secrets (sealed) | JSON export of graph: nodes, slots (portable fields only), links, kind set, flow revisions, snippets |
| Format | `.listo-snapshot` — signed tar + zstd; opaque payload | `.listo-template` — signed tar + zstd; payload is `template.json` + optional assets |
| Identity check on restore | Must match source `device_id` or be explicitly downgraded to a template | No device check; path-based merge |
| Portability | **None.** Same device only. | **Full.** Any device running a compatible agent version. |
| Typical size | MB to GB | KB to MB |
| Typical cadence | Hourly / daily / pre-apply / pre-reset | On edit, on release, on fleet push |
| Versioned | Agent binary + schema version pinned in manifest | `spi` major version pinned; forward-compatible within major |
| Canonical use cases | Disk died; OS reinstall; pre-OTA safety net; ransomware rollback | "Build on one edge, ship to 100"; edge → cloud promotion; snippet library; project templates; CI fixtures |

### 1.1 — Why this split is non-negotiable

If you try to make one bundle do both jobs you end up either:

- **Leaking secrets.** A "portable backup" that includes sealed claim tokens
  and per-device credentials either refuses to restore elsewhere (making it not
  portable) or exposes them (making it a security incident).
- **Losing state on DR.** A "portable backup" that strips runtime state isn't
  a real disaster recovery — it can't bring back history, time-series, or
  in-flight workflow cursors.
- **Reaching for MAC binding or global UUIDs** to paper over the mismatch.

Keeping the two bundles structurally different — different file extensions,
different headers, different REST endpoints, different CLI verbs — makes the
mistake hard to make in the first place.

### 1.2 — Bundle envelope (shared)

Both bundle types share the same outer envelope — the format listod already
defines for `.listo` OTA bundles ([listod/SCOPE.md](../../../listod/SCOPE.md)).
Reusing it means one verification codepath, one key infrastructure, one
signing discipline.

```
<bundle>.listo-{snapshot,template}   # tarball
  manifest.json                      # see §1.3
  manifest.sig                       # ed25519 over manifest.json
  payload.tar.zst                    # the actual data
  payload.sha256                     # hash of payload.tar.zst, covered by manifest
```

**Verification order** (identical to OTA bundles): subject → agent version gate
→ signature → hash → unpack. Any failure = reject; no partial apply.

### 1.3 — Manifest fields

```jsonc
{
  "bundle_kind": "snapshot" | "template",
  "bundle_version": 1,                  // envelope format version
  "subject": "agent-state",             // distinguishes from OTA "agent" / "listod" subjects
  "created_ms": 1735689600000,
  "created_by": { "device_id": "...", "user": "...", "tool": "agent@0.42.1" },

  // Snapshot-only
  "source_device_id": "dev_7x9k...",    // required for snapshot
  "source_hostname": "edge-boiler-03",  // advisory, never trusted for identity
  "agent_version": "0.42.1",            // exact version that produced the dump
  "schema": { "sqlite": 47, "postgres": 47 },
  "dumps": { "sqlite": "state.sqlite.zst", "postgres": "pg.dump.zst" },

  // Template-only
  "spi_major": 1,                       // contract compatibility gate
  "kinds_required": ["sys.logic.function", "com.listo.mqtt.broker", ...],
  "kind_versions": { "sys.logic.function": "1.2.0", ... },
  "root_path": "flows/boiler-1",        // null = whole graph
  "node_count": 312,
  "contains_snippets": true,

  // Shared
  "payload_sha256": "...",
  "encryption": { "scheme": "age-x25519", "recipients": [...] } // optional
}
```

The manifest is small, human-readable, and covered by the signature. Diagnostic
tools inspect it without unpacking; restore decisions (version gate, kind
availability, device match) are made against it alone.

---

## 2 — The portability field on `KindManifest`

### 2.1 — The problem

Template export has to decide, for every slot on every node, whether the value
travels or stays behind. Without a rule, every new kind becomes a judgment call
— and the wrong call either leaks secrets (`api_key` exported into a template
that ends up in a git repo) or breaks the restore (`last_seen_ms` exported,
causing a replica to look alive before it's joined the network).

### 2.2 — The rule

Add one field to `KindManifest` (in [contracts/spi](../../../contracts/spi/)),
per slot:

```rust
enum Portability {
    /// Logical configuration. Exported to templates. Round-trips cleanly.
    /// Default for config slots (kind config, node wiring, static options).
    Portable,

    /// Local to this device. Excluded from templates. Included in snapshots.
    /// Use for: last_seen_ms, runtime counters, assigned UUIDs, local paths,
    /// provider-session cursors, rate-limit state.
    /// Stripped values are left null on template import; the kind's init
    /// hook regenerates them on first tick (same path as a freshly created
    /// node). Kinds whose `Device` slots are required-non-null at validation
    /// time must supply an init default — a template import that leaves a
    /// required slot null is a kind-authoring bug, not an import bug.
    Device,

    /// Credentials. Excluded from templates. Included in snapshots but only
    /// in the sealed section (age/KMS encrypted at export time). Never in
    /// plaintext on disk outside a live process.
    /// Use for: api_key, oauth_token, private_key, password, mqtt_credentials.
    Secret,

    /// Derived from other slots at runtime. Excluded from both. Regenerated
    /// on first tick after restore.
    /// Use for: computed status, cached joins, query results.
    Derived,
}
```

Default for slots is `Portable` — you have to opt *out* of travelling. This is
the safer default: if a kind author forgets to classify, the slot goes into
templates and breaks in a loud, visible way (e.g. a secret shows up as
plaintext in review). The opposite default (secret by default) would silently
strip config and produce working-but-empty imports nobody notices.

### 2.3 — Enforcement

A contracts/spi change per [VERSIONING.md](VERSIONING.md). Add-only within a
major. Enforced in four places:

1. **`kinds register`** — reject a manifest whose slot schema has no
   `portability` for any slot. No implicit defaults at the edge.
2. **Static lint at register-time and in CI** — a slot whose name matches
   `(?i)(key|token|secret|password|credential|apikey|private[-_ ]?key|salt|nonce)`
   and is classified anything other than `Secret` is a **hard reject** at
   `kinds register`, not a warning. This catches the single most dangerous
   mistake — a forgotten classification shipping a credential into a
   template as plaintext — without relying on someone reading the import
   diff. The pattern is tuned to minimise false positives; kinds that
   legitimately trip it (e.g. `token_bucket_size`) annotate the slot with
   `#[portability_lint(allow = "not-a-credential")]` so the exception is
   explicit and auditable. Template export additionally scans *values* for
   high-entropy strings in `Portable` slots and fails export on hits
   pending operator override; this is belt-and-braces, not the primary
   defence.
3. **Template export** — strips `Device` / `Derived`, sealed-section encrypts
   `Secret`, includes `Portable` plaintext. Export fails if a `Secret` slot
   has no `encryption.recipients` configured (see §5). The `Portable`
   default is safe *only in combination with* rule 2; without the name-based
   lint, the default would leak on first authoring mistake.
4. **Snapshot export** — includes everything. `Secret` still goes through the
   sealed section so a snapshot tarball is not plaintext-credentials on disk.

### 2.4 — Authoring guidance

Every kind author reads one rule: **if it wouldn't make sense on another
physical device, it's `Device`. If losing it would be a security incident,
it's `Secret`. If a fresh tick would recompute it, it's `Derived`. Otherwise
it's `Portable`.**

Worked examples:

| Slot | Portability | Why |
|---|---|---|
| `mqtt_broker.host` | Portable | Logical config |
| `mqtt_broker.credentials` | Secret | Credential |
| `mqtt_broker.last_connected_ms` | Device | Runtime state |
| `mqtt_broker.subscribed_topic_count` | Derived | Recomputed on tick |
| `flow.enabled` | Portable | Logical config |
| `history.config.retention_days` | Portable | Logical config |
| `device.uuid` | Device | Locally allocated |
| `ai_session.conversation` | Device | Per-device runtime history |

This classification lives on the manifest — not in a side file, not in export
code — so a block author writing a new kind declares portability in the same
place they declare the slot's type. One source of truth.

---

## 3 — Import-conflict UX

Applies to templates only. Snapshots overwrite the device's own state by
definition — there's no "conflict" because the restoring device is supposed
to *become* the snapshotted device.

### 3.1 — The three strategies

When a template is imported and a path already exists on the target, the
operator picks one of three strategies. Same mental model as `git merge`,
`kubectl apply`, `terraform plan`.

| Strategy | Behaviour | Use when |
|---|---|---|
| **`namespace`** | Prefix everything under the root path with a tag: `flows/boiler-1` → `flows/imports/2026-04-22-boiler-1`. No existing node is touched. | Importing a snippet library; sharing across projects; keeping multiple versions side-by-side. Always safe. |
| **`merge`** | Per-path classification: *new* (add), *identical* (skip), *conflict* (present diff, require per-item decision or `--on-conflict {theirs,ours,fail}`). Runtime state preserved where kind and path match. | Edge → cloud promotion; upgrading a device with a new template revision. The one you use most. |
| **`overwrite`** | Replace-by-path. Any node at a matching path is deleted and re-created from the template. Runtime state (`Device` slots, history) cleared. | Rolling back a bad edit from a known-good template; initial fleet provisioning. Gated by **G2** — owner-token plus a physical-act proof ([listod/README.md § API surface](../../../listod/README.md)); same grade as factory-reset and self-update, and enforced by the same middleware. |

Default is `namespace`. The operator opts into `merge` and doubly opts into
`overwrite`. No strategy has a `--force` equivalent that silently combines
them.

### 3.2 — Preview before apply

Every import is a two-step: `preview` returns a plan, `apply` executes it.
This is GitOps-standard (`terraform plan` → `terraform apply`, `kubectl
diff` → `kubectl apply`). The plan is a typed, inspectable document:

```jsonc
{
  "strategy": "merge",
  "target_root": "flows/boiler-1",
  "actions": [
    { "op": "create", "path": "flows/boiler-1/heartbeat", "kind": "sys.logic.heartbeat" },
    { "op": "update", "path": "flows/boiler-1/setpoint",  "slot": "value", "from": 65, "to": 72 },
    { "op": "skip",   "path": "flows/boiler-1/alarm",     "reason": "identical" },
    { "op": "conflict", "path": "flows/boiler-1/schedule", "local_rev": 14, "incoming_rev": 9,
       "diff": { ... } }
  ],
  "kinds_missing": [],
  "warnings": ["3 Secret slots in template are unsealed; imported nil"],
  "portable_total": 312, "device_stripped": 47, "secret_stripped": 3, "derived_stripped": 19
}
```

The UI renders this diff; CI systems gate `apply` on it (exit non-zero if
`conflict.count > 0` and no resolution is provided). No silent merges.

**Missing-kinds is a hard fail at preview.** If `kinds_missing` is non-empty
— the template references a `KindManifest` the target agent hasn't
registered, or has registered at an incompatible version — `preview`
returns `status: "blocked"` with the missing kinds and their required
versions, and `apply` refuses outright. There is no `--force` to import
nodes of unknown kinds; the graph would fail validation on the first tick
and the restore would look partially-successful. The operator resolves by
installing the missing block (`agent blocks install <id>`) or upgrading the
agent, then re-running preview. Same discipline as `terraform apply`
against a missing provider.

### 3.3 — Conflict resolution

On `conflict` actions the operator provides resolutions:

- **`theirs`** — take the incoming template's value.
- **`ours`** — keep the local value.
- **`merge-slot`** — for container slots (lists, maps), take per-key union /
  diff. Requires the slot to declare `mergeable: true` on its schema (a
  sibling field to `portability` on the slot definition in `KindManifest`,
  added in the same contracts/spi change as §2.2). Defaults to `false`;
  `merge-slot` against a non-mergeable slot is rejected at `preview` time.
  Only meaningful for list / map / set slot types — scalar slots cannot
  declare it.
- **`fork`** — namespace the incoming node and keep both. Equivalent to
  locally overriding the merge strategy to `namespace` for that subtree.

Resolutions can be supplied inline in the apply request or via an edited plan
file (the `preview` JSON with `resolution` fields filled in). The edited-plan
path is the one CI uses.

### 3.4 — Atomicity

`apply` is transactional at the graph level: either every action lands or
none do. Implementation uses the existing flow-revision machinery — a template
apply is a single revision with a descriptive commit message (`template:
apply <bundle_id> strategy=merge`). Rollback is a single revision revert.
Same undo story as manual edits; no new concepts.

---

## 4 — The `device_id` check (snapshot only)

### 4.1 — Why not MAC, hostname, or a new UUID

Floated and rejected:

- **MAC address.** Ephemeral in VMs/containers; changes on NIC replacement
  (the exact DR scenario this is meant to solve); ambiguous on multi-NIC
  hosts; actively prevents restore on replacement hardware. Fails every real
  use case.
- **Hostname.** Operator-mutable; collides in fleets where 100 edges share a
  naming convention; not a security boundary.
- **A new global UUID.** Creates a second identity system parallel to the
  claim ID listod already owns. Two sources of truth drift.

### 4.2 — Use listod's claim

listod already establishes device identity at first boot
([listod/SCOPE.md](../../../listod/SCOPE.md): `claim.pending` → `claimed.json`
with `owner_token`). Reuse that identity — but derive `device_id` from a
stable anchor, *not* from the rotating token.

Extend `claimed.json` with one new field at claim-epoch creation:

```jsonc
{
  "owner_token_hash": "...",          // rotates on token rotation
  "claim_epoch": 3,                   // bumps on factory-reset
  "claim_id": "ci_9f3e...a21",        // 32 bytes OsRng, base64url. Set once
                                      // per claim_epoch. NEVER rewritten on
                                      // owner_token rotation.
  ...
}
```

Then:

```
device_id = SHA-256("listo-device-id-v1" || claim_epoch || claim_id)
```

`owner_token_hash` is deliberately *not* an input. `owner_token` is an
authentication credential; rotating it is routine operational hygiene and
must not change device identity. `claim_id` is the identity anchor,
generated once at claim-epoch creation and immutable for the life of that
epoch.

Properties:

- **Token rotation preserves identity.** Rotating `owner_token` is a no-op
  for `device_id`. Snapshots from before and after rotation restore on the
  same device.
- **Factory-reset changes identity.** On `medium` or `factory` reset,
  listod mints a new `claim_id` *and* bumps `claim_epoch`. The `device_id`
  changes — correctly, because a factory-reset device is a new device for
  backup purposes.
- **Hardware replacement preserves identity.** Operator re-claims on the
  replacement box using the same `claim_id` (carried in the claim bundle);
  the new device computes an identical `device_id` and snapshots restore
  cleanly. This is the canonical DR flow.
- **Public and deterministic.** Agents compute it the same way on every
  tier; fleet services compute it without touching the token hash.
- **Derivation domain-separated.** The `"listo-device-id-v1"` prefix
  prevents cross-use of this hash anywhere else (ADR-style future-proofing
  if we ever derive other per-device identifiers from the same inputs).

### 4.3 — The check

On snapshot restore:

```
target.device_id == manifest.source_device_id   → proceed
target.device_id != manifest.source_device_id   → refuse, unless
                                                   --as-template is passed
```

`--as-template` (API: `?as_template=true`) downgrades a snapshot restore to
a template import: the tool re-reads the snapshot, strips `Device`,
`Secret`, *and* `Derived` slots (i.e. everything except `Portable`), skips
DB dumps entirely, reconstructs a `template.json` in memory, and runs the
template-import codepath. This is the one sanctioned
way to migrate a snapshot onto a different device, and it does so with eyes
open: the operator sees the `device_stripped: N, secret_stripped: M` counts
in the preview.

Three outcomes — clear, loud, fail-safe:

1. Same device, signature valid → full restore, minutes.
2. Different device, no flag → refused; error points at `--as-template`.
3. Different device, `--as-template` → clean template import; no runtime
   state carried across.

### 4.4 — What a restore actually does

For a snapshot restore on the matching device:

1. Verify envelope (signature, hash, agent version, schema version).
2. Drain the live agent (SIGTERM-equivalent; in-flight messages flushed).
3. Back up the *current* state to a rollback bundle in
   `var/backups/pre-restore-<ts>.listo-snapshot` — automatic, non-negotiable.
   Same rollback guarantee as OTA.
4. Restore databases per §4.5 (SQLite file-swap; Postgres staged restore
   with extension/hypertable handling).
5. Replay the audit log forward if the snapshot is older than the rollback
   bundle and the operator passed `--replay-audit-since=<ts>` (optional,
   advanced).
6. Restart the agent; run `kinds verify` and a health-check sweep before
   accepting traffic.

A failed restore triggers automatic rollback to the pre-restore bundle. Same
discipline as OTA: no half-applied state, no manual recovery path needed.

### 4.5 — Database restore specifics

The "dump and restore" line in §4.4 hides real complexity. Spelled out:

**SQLite (edge / standalone tier).**

- Dump is a copy of the database file taken against an online WAL using
  `sqlite3_rsync` or `VACUUM INTO` — both produce a consistent file
  without quiescing writes. Dump is verified with `PRAGMA integrity_check`
  before bundling.
- Restore is a file-swap: write to `state.sqlite.new`, fsync, rename over
  `state.sqlite` under the agent-stopped window, `PRAGMA integrity_check`
  on the restored file, fail the restore if non-OK.
- Schema version (`PRAGMA user_version`) must equal `manifest.schema.sqlite`.
  Mismatch = refuse; operator upgrades the agent to the version named in
  the manifest or exports a new snapshot from the current version.

**Postgres (cloud tier; shared edge tier in some deployments).**

Cannot be treated as a file-swap. Restore happens in-database:

1. **Dump format.** `pg_dump --format=custom --no-owner --no-privileges
   --section=pre-data,data,post-data` produces a single archive suitable
   for `pg_restore`. Custom format is required for per-table restore,
   parallel restore, and selective object handling.
2. **Extensions first, separately.** Extensions (`timescaledb`, `pgcrypto`,
   `uuid-ossp`, etc.) are re-created from a manifest-embedded
   `extensions.sql` *before* `pg_restore` runs. `pg_dump` does not portably
   capture extension state; dumping and restoring into a database without
   the extension pre-installed silently drops the typed columns.
3. **TimescaleDB hypertables.** If the deployment uses Timescale (see
   [STORAGE-TIERS.md](STORAGE-TIERS.md) — `slot_timeseries` is the canonical
   consumer), dumps must use the Timescale-aware path:
   `timescaledb_pre_restore()` + `pg_restore` +
   `timescaledb_post_restore()`. The dump is taken with the Timescale-aware
   pg_dump flags (`--jobs=N`, Timescale's own `ts_insert_blocker` trigger
   disabled during dump). Skipping this loses chunk boundaries, continuous
   aggregates, and compression policies. `data-backup` owns this logic; it
   is not optional.
4. **Staged restore.** Restore into a `__restore_staging` schema, validate
   (row counts, checksum of a known canary table, extension presence),
   then rename-swap: rename current schemas to `__restore_rollback`,
   rename staging to live. Swap is transactional via
   `BEGIN; ALTER SCHEMA … RENAME …; COMMIT;`. Failure at any point leaves
   `__restore_rollback` intact for manual recovery.
5. **Cross-schema FKs and views.** The rename-swap works only if every FK
   and view references objects by unqualified name or by live-schema name.
   `data-postgres` migrations enforce this convention today; the restore
   path assumes it. Any migration that introduces a cross-schema FK to a
   fixed-name schema breaks restore and must be rejected in review.
6. **Sequences and identity columns.** `pg_restore` resets sequences to
   post-data values; we additionally bump every identity sequence by
   `10_000` on restore to leave headroom against any in-flight writes the
   drain step missed. Non-negotiable: a restored Postgres that hands out
   already-used IDs corrupts the graph silently.
7. **Continuous WAL / PITR.** If WAL archiving is configured, the
   snapshot manifest records the archive LSN at dump time; restore can
   optionally replay WAL forward to a requested timestamp
   (`--recover-to=<ts>`). This is the PITR story flagged in §9 as
   partially-deferred; the hooks exist, the UX is thin.

If the deployment doesn't use Timescale, steps 3 is a no-op but the
extension-first rule (step 2) still applies — `pgcrypto` alone is already
enough to silently corrupt a restore without it.

---

## 5 — Security

### 5.1 — Signing

Both bundle types are signed with the same ed25519 infrastructure as OTA
bundles. Two signing roles, with different guarantees:

- **Device-key signature (always present on snapshots).** Signed by the
  producing agent with its *device key* (provisioned at claim time).
  Proves: this snapshot came from this device, unmodified since dump. The
  device key lives on the box, so this signature does **not** protect
  against a compromised agent — a malicious agent can sign whatever it
  wants. Its job is authenticity and in-transit integrity.

- **Backup-service co-signature (optional, required for immutable-storage
  guarantees — see §5.3).** Signed by an external signer (fleet backup
  service, KMS identity, or an air-gapped signing box) whose key is
  **not** resident on the producing device. The backup service verifies
  the device signature, inspects the manifest (source device, age,
  expected cadence), and counter-signs. A snapshot without this
  counter-signature is valid for local DR but is not eligible for
  immutable-storage ransomware-rollback guarantees (§5.3).

Template bundles are signed by whatever key the publisher has (dev laptop,
CI system, fleet service). The verifier is policy-configured per subject
— `snapshot` bundles only accept the matching device key by default (plus
backup-service co-sign if policy requires it); `template` bundles accept
any key on the trust list.

### 5.2 — Encryption

`Secret` slots are sealed in an age/X25519 envelope inside the payload, with
the recipient public keys listed in `manifest.encryption.recipients`. For
snapshots: recipient is the device's own key, so the bundle is useless off-
box even if exfiltrated. For templates containing secrets (rare; usually
you'd template the non-secret config and inject secrets via
[AUTH.md](AUTH.md) at deploy time): recipients are the target devices' public
keys or a KMS identity. A template with `Secret` slots and no recipients
fails export — loud, not silent.

### 5.3 — Immutable + signed at rest

Recommendation, not enforcement (fleet-level policy): backup storage should
be append-only or object-lock (S3 Object Lock, WORM NAS, immutable Borg
repos). This is **half** of the ransomware-rollback story; it only works
in combination with off-box signing.

The full story, in order of strength:

1. **Object-lock storage alone.** Attacker who compromises the agent
   cannot delete or overwrite prior snapshots on the bucket. They *can*
   write new (malicious) snapshots that appear legitimate, because the
   agent's device key is on the box and is all that's needed to sign. A
   restore tool picking "the most recent valid snapshot" walks straight
   into the trap. Useful but not sufficient.

2. **Object-lock + backup-service co-signature (§5.1).** Restore policy
   requires the co-signature; the co-signing key is off-box and under
   separate trust. A compromised agent produces device-signed snapshots
   that the backup service refuses to counter-sign once anomalous
   behaviour is detected (or, simpler: a time-delay policy — co-sign only
   after N hours, during which an alert can catch the compromise). This is
   the configuration that actually delivers ransomware rollback.

3. **Object-lock + co-signature + periodic external verification.** A
   separate auditor process (distinct identity again) periodically
   fetches, verifies, and re-signs "attested good" snapshots. Belt and
   braces; recommended for regulated verticals.

The earlier draft of this doc claimed ransomware assurance from signing
alone. That was wrong — the device key is on the device. Re-stated: **only
off-box co-signing delivers the ransomware-rollback property.** Deployments
that don't run a backup service should treat snapshots as DR-only and
accept that a sufficiently-compromised agent can produce convincing-looking
malicious snapshots.

### 5.4 — SBOM + provenance

Snapshot manifests embed the agent's `agent_version` and, optionally, the
full SBOM hash of the running binary (SLSA-style provenance attestation).
Templates embed `spi_major` + `kind_versions`. Both are covered by the
signature. This is the bare minimum for 2025/26 supply-chain hygiene — not
optional in regulated verticals.

---

## 6 — Where the code lives

Follows the standard layering ([CODE-LAYOUT.md](../../../SKILLS/CODE-LAYOUT.md),
Q4 of [HOW-TO-ADD-CODE.md](HOW-TO-ADD-CODE.md)).

```
contracts/spi/                         # Portability enum; bundle manifest types
agent/crates/domain-backup/            # Pure logic: export/import, template vs snapshot,
                                       #   conflict resolver, device_id derivation.
                                       #   No HTTP, no SQL.
agent/crates/data-backup/              # DB dump helpers (SQLite file copy + integrity check;
                                       #   pg_dump / pg_restore orchestration). Snapshot only.
agent/crates/transport-rest/backup.rs  # Thin handlers: POST /api/v1/backup/{snapshot,template}/
                                       #   {export,import,preview}. Each <20 lines.
agent/crates/transport-cli/            # `agent backup snapshot {export,import}`,
                                       #   `agent backup template {export,import,preview}`
agent-client-{rs,ts,dart}/             # Client surfaces mirror REST
listod/src/hook.rs                     # Unchanged. Pre-apply/pre-reset hook shells out to
                                       #   `agent backup snapshot export`. listod does not
                                       #   grow.
```

Fleet replication (1 → N edges) is a *downstream* concern:

```
agent/crates/transport-fleet-zenoh/    # Existing. Carries template bundles as payloads on a
                                       #   fleet topic; target agents import via their own
                                       #   REST API. No new service.
```

No new repo. No new daemon. Backup is domain logic the agent already has the
surfaces for.

### 6.1 — Backup is independent of artifacts

**`data-backup` has no dependency on `ArtifactStore` and no dependency on any
`data-artifacts-*` crate.** This is load-bearing: a minimal single-device
deployment (a Raspberry Pi running a standalone appliance, an air-gapped
edge, a developer laptop) must be able to take, keep, and restore snapshots
without linking any object-store code.

`data-backup` writes to any `io::Write` — a local path, stdout, a pipe, or
(when `artifacts-s3` / `artifacts-local` is compiled in and `domain-artifacts`
adapts the sink) an `ArtifactStore`-backed writer. The bundle format and
the writer are two separate concerns. The handoff is a file or a stream;
nothing in `domain-backup` imports `ArtifactStore`.

### 6.2 — Three operating modes

| Build | What works | What doesn't |
|---|---|---|
| **Backup only** (no `artifacts-*` feature) | `agent backup snapshot export --to /var/backups/foo.listo-snapshot`, local-file restore, listod pre-apply hook, CLI import/export, `--as-template` downgrade, small fleet template push (≤ `smallest_edge_ram/10`) over Zenoh | Presigned upload to cloud, `--to s3://…`, multi-MB fleet template push, 3-2-1 off-site copy, ransomware-rollback guarantee (needs off-box co-sign + immutable-at-rest) |
| **Backup + `artifacts-local`** | Everything above, plus "cloud" = a local directory. Useful for air-gapped sites, single-node deployments, dev loops | Real S3 / off-site replication |
| **Backup + `artifacts-s3`** | Full story: edge → cloud snapshot upload, cloud → edge template push, Object Lock immutability, multi-tenant buckets, 3-2-1 rule, ransomware rollback (with co-sign per § 5.3) | — |

**A Raspberry Pi running standalone gets automatic snapshots to
`/var/backups/` with neither feature flag enabled.** That's the whole point
of the layering.

### 6.3 — What you lose going minimal

Re-read § 5.3: the ransomware-rollback guarantee requires off-box co-signing
*and* immutable-at-rest storage. In backup-only mode, bundles sit on local
disk, and a sufficiently-compromised agent can rewrite its own backups.
That's still the majority of real-world backup value — disk died, rolled
back a bad edit, pre-OTA safety net — just not the ransomware story.

Also lost without artifacts:

- **Fleet template push** for bundles larger than the fleet-payload ceiling
  (~`smallest_edge_ram/10` per [ARTIFACTS.md § 1](ARTIFACTS.md)). Small
  templates fleet-push fine without artifacts; multi-MB ones don't.
- **The 3-2-1 rule's "one off-site" copy** (§ 7 below) — local-only is
  one-copy-two-media at best.

### 6.4 — The integration seam (where the two systems meet)

When both `data-backup` and an `artifacts-*` feature are compiled in,
*something* has to take the `io::Write` stream `data-backup` produces
and hand it to `ArtifactStore::put` as a `ByteStream`. The invariant
is strict:

- `data-backup` has **no** dependency on `ArtifactStore`.
- `data-artifacts-*` has **no** dependency on `data-backup` or any
  bundle type.

**The wiring lives in the handler layer, not in a new crate.** The
REST and CLI handlers in `transport-rest/src/backup.rs` and
`transport-cli/src/commands/backup.rs` already import both
subsystems (they're the policy boundary where "where the bytes go"
is decided — see [HOW-TO-ADD-CODE.md Rule I](../../../HOW-TO-ADD-CODE.md)).
The handler creates a `tokio::io::duplex()` pair, spawns
`data-backup::export` writing to the write half, and feeds the read
half as a `ByteStream` to `ArtifactStore::put`. No new crate, no
new trait — ~15 lines of glue per handler. Tested end-to-end as § 6.6.

In code layout terms:

```
agent/crates/transport-rest/src/backup.rs       # one seam (REST)
agent/crates/transport-cli/src/commands/backup.rs # second seam (CLI)
    ↓ feature-gated by artifacts-*
        tokio::io::duplex → domain-artifacts::upload_stream
```

A thin helper — `domain-artifacts::upload_stream(store, key, reader)
-> Result` — wraps the duplex dance so both handlers share the same
five lines. That helper is the closest thing to an "integration
point" and lives in `domain-artifacts` (which already imports neither
`data-backup` nor bundle types — it just consumes any `AsyncRead`).

### 6.5 — CLI flag behaviour under feature combinations

`agent backup snapshot export --to <destination>` accepts three
destination forms. Which are advertised depends on compile-time
features:

| Build | `--to /path/...` | `--to s3://...` | `--to local://...` |
|---|---|---|---|
| Backup only (no `artifacts-*`) | ✅ shown in `--help` | ❌ not in `--help`; rejected at parse time | ❌ same |
| `artifacts-local` | ✅ | ❌ same | ✅ shown |
| `artifacts-s3` | ✅ | ✅ shown | ❌ not unless also `artifacts-local` |

The flag surface itself is gated — `clap` value parsers for
destination schemes are registered behind `#[cfg(feature = "...")]`.
A user on a minimal build running `agent backup snapshot export --to
s3://…` gets:

```
error: invalid value 's3://bucket/key.listo-snapshot' for '--to <DEST>':
  scheme 's3' is not supported by this build.
  Rebuild with `--features artifacts-s3` or use a local path.
```

Not a runtime panic, not a silent fail. The help text only lists
schemes that work. This matches the Cargo-feature philosophy used
elsewhere (a feature that isn't compiled in doesn't exist from the
user's perspective).

### 6.6 — Capability advertisement

Pushing back on the symmetric-with-artifacts framing: **backup is
unconditionally compiled.** There is no `backup: null` mode, no
`NullBackupStore` in `spi`, no build that ships without the ability
to produce a local-file snapshot.

Why the asymmetry: artifacts is a *distribution* subsystem whose
absence is a real operating mode (standalone Pi, no cloud). Backup
is *core domain logic* — every agent that has a database has a
backup subsystem, because every agent can dump its DB to a file.
Shipping a build with backup disabled would silently degrade DR for
no engineering benefit; the DB-dump code is small, the bundle logic
is pure, and the feature costs nothing to carry.

The agent therefore **always advertises `backup.v1`** in its
capability map. Clients never see `backup.null.v1`. The dimensions
that *do* vary — and that clients must check — are:

| Capability | Meaning |
|---|---|
| `backup.v1` | Always present. Local-file export/import works. |
| `artifacts.<id>.v1` | Absent / null / real. Gates cloud upload UIs. |
| `fleet.<id>.v1` | Absent / null / real. Gates fleet-template-push UIs. |

Studio's backup page asks "does this agent have `artifacts.s3.v1`?"
to decide whether to show the "upload to cloud" button — not
"does it have backup?". The backup button is always there because
backup is always there.

### 6.7 — Routing: fleet vs artifacts for template push

Pushing a template from edge A to edges B…N has two possible
transports:

- **Fleet-carried** — the template bytes ride as the payload of a
  Zenoh control message. Cheap, no artifact store needed, but
  bounded by the fleet payload limit (~`smallest_edge_ram/10`).
- **Artifact-carried** — the template uploads to the tenant bucket
  via `artifacts-s3`, the fleet message carries only a key + hash
  + signature, receivers fetch via `presign-download`. Unbounded.

**The routing decision lives in the handler**, not in domain code.
`transport-rest/src/backup.rs` (and the CLI equivalent) measures the
serialised template size and picks:

```
if size <= fleet_payload_ceiling and artifacts store is null:
    publish via FleetTransport (direct payload)
elif artifacts store is present:
    upload via ArtifactStore, publish key + hash via FleetTransport
else:
    error: "template exceeds fleet payload limit; rebuild with artifacts-s3"
```

This keeps `domain-backup` transport-agnostic (no `FleetTransport`
dep, no `ArtifactStore` dep) and puts the policy decision where
policy decisions go ([HOW-TO-ADD-CODE.md Rule I](../../../HOW-TO-ADD-CODE.md)).
Same pattern as the handler deciding between `io::Write` to a local
file and `ArtifactStore::put` for snapshot export (§ 6.4).

---

## 7 — Cadence, retention, drills

### 7.1 — Two retention concerns, two nodes

There are two distinct retention questions, and they're owned by
different nodes. Confusing them is the retention-drift bug
[ARTIFACTS.md § 10](ARTIFACTS.md) warns about.

| Node | Owns | Scope |
|---|---|---|
| `sys.backup.config` | **Cadence of creation.** When to take a snapshot (hourly / daily / pre-apply), what to include, minimum retention floor (e.g. "never delete snapshots younger than 24h"). | Per device. |
| `sys.artifacts.retention` | **Store-level expiry.** Per-prefix TTL for objects in the artefact store (`snapshots/…` → 30d, `templates/…` → forever, `firmware/…` → 180d). Drives the retention job + S3 lifecycle backstop. | Per tenant. |

They complement, they don't overlap: `sys.backup.config` says *take
one every hour and keep at least the last 24*; `sys.artifacts.retention`
says *delete anything older than 30 days from the store*. Neither
contradicts the other because `sys.backup.config`'s floor is
short-term ("enough to survive a bad day") and `sys.artifacts.retention`'s
ceiling is long-term ("don't pay for months of bytes").

**Conflict resolution rule:** when a snapshot falls inside the backup
floor *and* past the artifact expiry, the backup floor wins — the
retention job skips objects that `sys.backup.config` still considers
floor-protected. This is expressed as a predicate on the retention job
(§ 10.1 of ARTIFACTS.md) that joins against the receipts table on
`created_at`. In a minimal build with no artifacts subsystem,
`sys.artifacts.retention` is absent and `sys.backup.config` governs
alone — files in `/var/backups/` are pruned by `data-backup`'s own
local-file sweep based purely on the cadence node.

### 7.2 — Defaults

Operational defaults, per 2025/26 common practice:

- **Automatic snapshots:** hourly incremental (SQLite WAL archive + pg
  continuous WAL archiving), daily full, 7-day hot + 30-day cold retention.
  Configurable per device via a `sys.backup.config` node (yes — backup
  settings are a node, see Rule A).
- **Pre-change safety:** listod's existing pre-apply / pre-reset hook
  captures a snapshot. Retained until the next apply succeeds.
- **3-2-1 rule:** three copies, two media, one off-site. Implemented as
  local disk + fleet-pull to a central store + S3 Object Lock replica.
  Policy lives in [FLEET-TRANSPORT.md](FLEET-TRANSPORT.md); agents expose
  the export endpoint and don't care where the bytes end up.
- **RPO / RTO:** edge default RPO 1h / RTO 15min; cloud default RPO 5min /
  RTO 5min. Tunable.
- **Restore drills:** scheduled monthly dry-run — pull last snapshot into a
  scratch agent on a side port, run the `kinds verify` sweep, discard.
  "Untested backups don't exist" is policy, not slogan.

---

## 8 — Decision matrix (what do I actually run?)

| I want to… | Command | Bundle |
|---|---|---|
| Insure against disk failure on edge-03 | `agent backup snapshot export --to s3://…` (cron) | snapshot |
| Roll edge-03 back after a bad flow edit | `agent backup snapshot import <file>` (same device) | snapshot |
| Clone edge-03's configuration to 99 new edges | `agent backup template export --root /` → fleet push → N× `agent backup template import --strategy=overwrite` | template |
| Promote a tested project from edge to cloud | `agent backup template export --root flows/boiler-1` → cloud: `agent backup template import --strategy=merge` | template |
| Share a flow snippet with a colleague | `agent backup template export --root flows/boiler-1/heartbeat --as-snippet` | template |
| Migrate edge-03's config to replacement hardware | On new device: `agent backup snapshot import <file> --as-template` | snapshot → template downgrade |
| Pre-OTA safety net | listod calls the pre-apply hook; produces `var/backups/pre-apply-<ts>.listo-snapshot` automatically | snapshot |

---

## 9 — What this doc does not cover (yet)

Flagged for follow-up — resolve before the milestone that touches them.

- **Incremental template diffs.** Today a template apply is full-payload;
  for very large graphs you'd want `template diff A B` producing a
  minimal-action patch bundle. Design, not implemented.
- **Point-in-time restore (PITR) granularity.** Snapshot cadence gives
  hourly RPO; true PITR (replay to exact timestamp) needs WAL archiving
  hooks on both SQLite and Postgres tiers. `data-backup` has the seams;
  the operator story isn't written.
- **Cross-version template import.** Today: same `spi_major` only. Upgrade
  flow (import an old template into a new agent) needs schema-migration
  support analogous to the DB migration story. Probably a `template
  migrate` step that produces a new template targeting the newer `spi`.
- **Selective snapshot restore.** Restoring a single flow's time-series
  without nuking the rest. Needs tier-2 repo-level extract/apply APIs.
  Defer until a real use case forces it.
- **KMS integration.** Sealing `Secret` slots with a cloud KMS (AWS/GCP/
  Azure) instead of local age keys. Adds `encryption.scheme = "kms-*"`
  variants. Infrastructure, not logic.

---

## 10 — Summary

- **Two bundle types, never one.** Snapshot (device-bound DR) and template
  (portable logical config). Different extensions, different endpoints,
  different verbs.
- **Portability is a slot-level manifest field**, defaulting to `Portable`,
  declared by the kind author, and backed by a name-based credential lint
  that hard-rejects obvious classification mistakes at `kinds register`.
- **Imports are plan-then-apply**, with three explicit strategies and no
  silent merges. GitOps discipline.
- **Device identity is listod's claim**, hashed. No MAC, no hostname, no
  new UUID system.
- **Envelope, signing, rollback, verify-before-stage**: reuse listod's
  existing infrastructure. One format, one codepath, one set of keys.
- **Fleet replication is not backup.** It's template deployment over
  Zenoh, and it doesn't need new services — just the template API and
  the fleet transport that already exist.
