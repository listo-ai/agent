# /clients — Cross-Language Client Packages

One directory, one package per language. Same discipline as `/crates/` — small files, clear layers, each package has one job.

## Packages

| Package | Language | npm / crate |
|---------|----------|-------------|
| [`/ts`](./ts) | TypeScript | `@acme/agent-client` |
| `/rust` | Rust | _later_ |
| `/go` | Go | _later_ |
| `/python` | Python | _later_ |

## Policy

- **Versioning**: Three independent version numbers per client — see each package's `version.ts` / equivalent.
  1. Client package version (`package.json "version"`)
  2. Supported REST API version (`REST_API_VERSION`)
  3. Required host capabilities (`REQUIRED_CAPABILITIES`)
- **Testing**: Every client must pass the round-trip fixtures in [`/contracts/fixtures/`](./contracts/).
- **Release**: Clients are released independently from the agent binary. A client major bump is required when `REST_API_VERSION` changes.
- **No business logic in clients**: clients are wrappers around the wire protocol, not domain services.

## Contracts

`/contracts/` is the cross-language source of truth — JSON fixtures for messages, events, and capability manifests. A client passes these or it doesn't ship.
