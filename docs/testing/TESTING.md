# Testing Guide — Local Dev Environment

> **FOR LOCAL TESTING ONLY.**
> Every command in this document targets `localhost`. Do not run these against
> a production or staging environment. The configs under `dev/` are
> intentionally unsecured (no TLS, no auth enforcement) and are git-tracked
> for convenience.

---

## Two commands. That's it.

| Command | When to use |
|---------|-------------|
| `make run` | Everyday dev and single-agent testing |
| `make dev` | Testing edge ↔ cloud communication |

---

## `make run` — single edge agent

```bash
make run
```

Starts one edge agent on **http://localhost:8080** using `dev/edge.yaml` and
`dev/edge.db`. Plugins are loaded from `dev/edge-plugins/`.

This is the default. Use it for:
- Flow authoring and execution
- Plugin development
- REST API testing
- Anything that doesn't require a cloud agent

Health check:
```bash
curl http://localhost:8080/healthz
```

---

## `make dev` — cloud + edge side by side

```bash
make dev
```

Starts four processes via `dev/run.sh` (pure bash, no extra tools),
colour-coded logs, Ctrl-C stops all:

| Process      | Role  | Port  | Config            | DB             |
|--------------|-------|-------|-------------------|----------------|
| cloud agent  | cloud | 8081  | `dev/cloud.yaml`  | `dev/cloud.db` |
| edge agent   | edge  | 8082  | `dev/edge.yaml`   | `dev/edge.db`  |
| Studio       | —     | 3001  | → cloud agent     | —              |
| Studio       | —     | 3002  | → edge agent      | —              |

Use it for:
- Fleet transport and subject namespacing between agents
- Cloud-to-edge flow dispatch
- UI differences between cloud and edge Studio views

To start processes individually instead:
```bash
make run-cloud    # cloud agent → http://localhost:8081
make run-edge     # edge agent  → http://localhost:8082
make studio-cloud # Studio      → http://localhost:3001
make studio-edge  # Studio      → http://localhost:3002
```

---

## Wipe state

> **AI AGENTS: do NOT run this unless the user explicitly asks you to.**
> The databases contain the user's local graph state. Wiping them loses all
> nodes, flows, and configurations the user has created. Normal
> stop/restart (`Ctrl-C` → `make dev`) does not require a reset.

```bash
make dev-reset
```

Removes `dev/cloud.db`, `dev/edge.db`, and all staged plugins. Both agents
re-seed fresh graphs on next boot. Only needed when the schema has changed
or you want a completely clean slate.

---

## Common tasks

### Stage a plugin

```bash
cp -r plugins/com.acme.hello dev/edge-plugins/
curl -X POST http://localhost:8080/api/v1/plugins/reload   # make run
curl -X POST http://localhost:8082/api/v1/plugins/reload   # make dev (edge)
curl -X POST http://localhost:8081/api/v1/plugins/reload   # make dev (cloud)
```

### Inspect the graph

```bash
curl http://localhost:8080/api/v1/nodes
curl http://localhost:8080/api/v1/nodes/<id>/slots
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `overmind not found` | Install it (see above) or use the four manual targets |
| Port already in use | `pkill -f "target/debug/agent"` then retry |
| 404 on `/api/v1/nodes` | Wait ~2 s for boot seed; check logs for `graph seeded` |
| Plugin not appearing | `curl -X POST http://localhost:808x/api/v1/plugins/reload` |
| Stale binary | `cargo build --bin agent` then restart |

---

## What this environment covers

- Edge-to-cloud agent communication (fleet transport, subject namespacing)
- Plugin lifecycle on both roles
- Flow authoring and execution against locally running protocols
- REST API surface on both agents
- Studio UI differences between cloud and edge views

It does **not** cover:
- Multi-tenant NATS (later stage — see `STEPS.md`)
- TLS / mTLS between agents
- Auth (Zitadel) — agents run without auth enforcement locally

---

## Related docs

| Doc | What it covers |
|-----|----------------|
| [dev/README.md](../../dev/README.md) | Port map, layout, per-agent plugin staging |
| [docs/design/NEW-SESSION.md](../design/NEW-SESSION.md) | Project rules and doc index for AI coding sessions |
| [docs/design/TESTS.md](../design/TESTS.md) | Unit/integration test categories, CI gates |
| [Makefile](../../Makefile) | All available `make` targets |
