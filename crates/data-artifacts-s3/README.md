# data-artifacts-s3

S3-compatible [`ArtifactStore`](../../../contracts/spi/src/artifacts.rs)
implementation for the Listo agent. Backed by the
[`object_store`](https://docs.rs/object_store) crate's AWS backend.

## Target backends

One binary speaks to every credible S3-compatible endpoint:

| Backend | Use |
|---|---|
| **Garage** | Self-hosted reference. Pure-Rust, geo-distributed, AGPL, minimal ops. |
| **Cloudflare R2** | Managed, zero egress fees. Right default for Listo-hosted cloud. |
| **AWS S3** | Enterprise customers already on AWS. |
| **Backblaze B2** | Cheapest per-GB for cost-sensitive self-managed deployments. |

## Compatibility subset

Uses only the operations every backend implements: PUT (single +
multipart), GET (with ranges), HEAD, DELETE, LIST, presign, Object
Lock, lifecycle. No SSE-KMS, no Intelligent-Tiering, no S3 Inventory.
See [ARTIFACTS.md § 5.3](../../docs/design/ARTIFACTS.md).

A CI smoke test runs the full integration suite against a real Garage
instance and a real R2 bucket; passing both is the contract.

## Build-time gating

**This crate is not linked into standalone builds.** It's pulled in by
the parent agent crate only when the `artifacts-s3` feature is
enabled. A `fleet: null` / no-cloud deployment ships without
object_store or any S3 SDK dependency.

## Status

Scaffolding. Struct + trait impl with `todo!()` bodies. Not yet in
the workspace `Cargo.toml`.
