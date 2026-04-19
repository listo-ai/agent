# Platform

A generic, extensible, flow-based integration platform. Rust, one codebase,
runs on 512 MB ARM edge gateways and in the cloud. Users author flows in the
Studio; agents execute them against pluggable extensions.

---

## Quick start

```bash
make install        # install JS/TS dependencies (pnpm workspaces)
make run            # single edge agent → http://localhost:8080
make frontend       # Studio dev server → http://localhost:3000
```

Full cloud + edge topology (edge ↔ cloud communication):

```bash
make dev            # cloud agent :8081, edge agent :8082, two Studios
make dev-reset      # wipe dev/ databases and staged plugins
```

See [docs/testing/TESTING.md](docs/testing/TESTING.md) for the full local
testing guide.

---

## All make targets

```bash
make help
```

---

## Docs

| Doc | What it covers |
|-----|----------------|
| [docs/design/NEW-SESSION.md](docs/design/NEW-SESSION.md) | **Start here for any coding session.** Project rules, non-negotiables, task router. |
| [docs/design/OVERVIEW.md](docs/design/OVERVIEW.md) | Deployment profiles, build targets, capability matrix, memory budgets. |
| [docs/design/EVERYTHING-AS-NODE.md](docs/design/EVERYTHING-AS-NODE.md) | Core model — graph, nodes, slots, kinds, flows, wires. |
| [docs/design/RUNTIME.md](docs/design/RUNTIME.md) | Engine lifecycle, safe-state, simulation vs commissioning modes. |
| [docs/design/CODE-LAYOUT.md](docs/design/CODE-LAYOUT.md) | Crate structure, layer rules, naming conventions, anti-patterns. |
| [docs/design/UI.md](docs/design/UI.md) | Studio architecture — Tauri, Rsbuild, Shadcn, Module Federation. |
| [docs/design/AUTH.md](docs/design/AUTH.md) | Zitadel, JWT verification, JWKS caching, offline operation. |
| [docs/design/MCP.md](docs/design/MCP.md) | MCP server — tools, resources, prompts, auth, off-switch. |
| [docs/design/TESTS.md](docs/design/TESTS.md) | Test categories, CI gates, what not to test. |
| [docs/testing/TESTING.md](docs/testing/TESTING.md) | Local dev environment — how to build and start the stack. |
| [dev/README.md](dev/README.md) | Two-agent dev topology — port map, configs, plugin staging. |

---

## Repo layout

```
crates/         # Rust — Cargo workspace
  spi/          # extension.proto, JSON schemas, trait signatures
  data-*/       # data layer — SQLite / Postgres / TSDB
  domain-*/     # pure business logic
  transport-*/  # REST, gRPC, NATS, CLI, MCP
  engine/       # flow engine + node runtime
  apps/agent/   # the single binary (role selected at runtime)

clients/
  ts/           # TypeScript SDK
  rs/           # Rust SDK

frontend/       # Studio — Tauri + Rsbuild + React + Shadcn

dev/            # local two-agent dev configs + databases
plugins/        # first-party plugins (dev/testing)
docs/           # design docs and testing guides
```

Single pnpm workspace for JS, single Cargo workspace for Rust.
Edge / cloud / standalone is one binary — role and features are runtime/compile-time flags, not separate packages.
