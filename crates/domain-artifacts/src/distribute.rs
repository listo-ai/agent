//! Decide-to-fetch and decide-to-upload orchestration.
//!
//! Called by the fleet command receiver on incoming
//! `cmd.block.install` / `cmd.template.apply` messages, and by the
//! backup scheduler on snapshot-export completion.
//!
//! Flow A (cloud → edge): look up local cache → if miss, fetch via
//! `ArtifactStore::get` → verify → install.
//! Flow B (edge → cloud): request presigned URL over fleet → upload
//! via `ArtifactStore::put` → publish completion event.
//!
//! STATUS: scaffolding.

// TODO: pub async fn fetch_and_verify(
//           store: &dyn ArtifactStore,
//           cache: &ArtifactCache,
//           cmd: &InstallCommand,
//       ) -> Result<VerifiedArtifact, DistributeError>

// TODO: pub async fn upload_snapshot(
//           store: &dyn ArtifactStore,
//           presign_via: &dyn FleetTransport,
//           bundle: SnapshotBundle,
//       ) -> Result<UploadReceipt, DistributeError>
