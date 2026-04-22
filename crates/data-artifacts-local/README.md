# data-artifacts-local

Filesystem-backed
[`ArtifactStore`](../../../contracts/spi/src/artifacts.rs)
implementation. For developer loops, single-node deployments,
air-gapped sites, and integration tests.

## When to use

| Scenario | Fit |
|---|---|
| `mani run dev-single` development | ✅ Primary use |
| Integration tests against a real `ArtifactStore` | ✅ Tempdir root |
| Air-gapped on-prem deployment | ✅ When cloud storage is forbidden |
| Production multi-tenant cloud | ❌ Use `data-artifacts-s3` instead |

## Security caveat

"Presigned URLs" are path tokens the in-process REST handler redeems.
They are **not** cryptographic signatures against an external provider.
Do not deploy this backend where real tenant isolation is required —
there's no bucket-level enforcement, only application-level path
scoping.

## Build-time gating

Compiled out by default. Pulled in via the agent's `artifacts-local`
Cargo feature.

## Status

Scaffolding. Struct + trait impl with `todo!()` bodies. Not yet in
the workspace `Cargo.toml`.
