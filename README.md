# agent

Core platform — engine, graph, transports, block host, and the `agent` binary.
Rust, one codebase, runs on 512 MB ARM edge gateways and in the cloud.

Users author flows in [Studio](../studio); agents execute them against pluggable
[blocks](../blocks).

## Build & run

```bash
cargo build --bin agent
cargo test --workspace
```

## Dev servers

From the workspace root:

```bash
mani run dev-single                         # standalone :8080 + Studio :3000
mani run dev-cloud                          # cloud :8081 + Studio :3002
mani run dev-edge                           # edge  :8082 + Studio :3010
mani run dev                                # cloud + edge
HTTP_PORT=8083 STUDIO_PORT=3011 mani run dev-edge   # second edge instance
mani run kill-dev                           # kill all dev processes
```

## Structure

- `crates/graph`, `crates/engine` — core substrate
- `crates/domain-*` — business logic (9 domains)
- `crates/transport-*` — wire surfaces (REST, gRPC, NATS, Zenoh, CLI, MCP)
- `crates/data-*` — persistence (sqlite, postgres, tsdb)
- `crates/blocks-host` — block supervisor
- `apps/agent` — the binary

## Dependencies

- [`contracts`](../contracts) — wire types (`listo-spi`, `listo-ui-ir`)
- [`agent-client-rs`](../agent-client-rs) — used by integration tests

Part of the [listo-ai workspace](../).
