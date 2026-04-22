# domain-artifacts

Pure artefact-distribution logic for the Listo agent: decide-to-fetch,
verify, cache. No HTTP, no S3 SDK — backends live in
`data-artifacts-s3` / `data-artifacts-local` behind Cargo features.

See [agent/docs/design/ARTIFACTS.md](../../docs/design/ARTIFACTS.md)
for the full design. This crate owns:

- **verify** — integrity (SHA-256) + signature (ed25519) checks on
  fetched bytes. Mismatch = reject, no partial apply.
- **cache** — content-addressed local cache with LRU eviction.
- **distribute** — orchestration of fetch-verify-install (cloud→edge)
  and request-upload-publish (edge→cloud).
- **keys** — re-export of typed key constructors from `spi`.

Depends on the [`ArtifactStore`](../../../contracts/spi/src/artifacts.rs)
trait in `spi`; any backend satisfies the same contract.

## Status

Scaffolding only. Module skeletons + TODOs, no logic yet. Not wired
into the workspace `Cargo.toml` until the implementation PR.
