// STAGE-1 complete — bundle writing: tar + zstd + manifest + SHA-256

//! Bundle writer — produces `.listo-snapshot` / `.listo-template` files.
//!
//! The bundle envelope is a tar archive containing:
//! ```text
//! manifest.json    — signed metadata (see spi::backup::BundleManifest)
//! payload.tar.zst  — zstd-compressed inner tarball with the actual data
//! payload.sha256   — hex SHA-256 of payload.tar.zst
//! ```
//!
//! This module writes to any `io::Write` — a local file, stdout, or
//! (via the duplex integration seam) a stream into `ArtifactStore`.
//! See BACKUP.md § 6.1 + § 6.4.

use std::io::{self, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use spi::backup::BundleManifest;
use tracing::info;

use crate::error::BackupError;

/// Build a zstd-compressed tarball from files in `source_dir` and
/// return the SHA-256 hex digest of the compressed output.
///
/// `source_dir` is the staging directory containing files to pack
/// (e.g. `state.sqlite.zst`, `pg.dump.zst`). Every regular file in
/// the directory is included at the root of the inner tar.
pub fn compress_payload(source_dir: &Path, dest: &Path) -> Result<String, BackupError> {
    let dest_file = std::fs::File::create(dest)
        .map_err(|e| BackupError::Io(format!("create {}: {e}", dest.display())))?;

    let mut hasher = Sha256HashWriter::new(dest_file);
    {
        let zstd_enc = zstd::Encoder::new(&mut hasher, 3)
            .map_err(|e| BackupError::Io(format!("zstd init: {e}")))?;
        let mut tar_builder = tar::Builder::new(zstd_enc);

        for entry in std::fs::read_dir(source_dir)
            .map_err(|e| BackupError::Io(format!("read_dir {}: {e}", source_dir.display())))?
        {
            let entry =
                entry.map_err(|e| BackupError::Io(format!("dir entry: {e}")))?;
            let path = entry.path();
            if path.is_file() {
                let name = path
                    .file_name()
                    .ok_or_else(|| BackupError::Io("no filename".into()))?;
                tar_builder
                    .append_path_with_name(&path, name)
                    .map_err(|e| BackupError::Io(format!("tar append: {e}")))?;
            }
        }

        let zstd_enc = tar_builder
            .into_inner()
            .map_err(|e| BackupError::Io(format!("tar finish: {e}")))?;
        zstd_enc
            .finish()
            .map_err(|e| BackupError::Io(format!("zstd finish: {e}")))?;
    }

    let sha256_hex = hasher.finalize_hex();
    info!(
        path = %dest.display(),
        sha256 = %sha256_hex,
        "payload compressed"
    );
    Ok(sha256_hex)
}

/// Write the outer bundle envelope (the top-level tar that users see
/// as `*.listo-snapshot` or `*.listo-template`).
///
/// `payload_path` is the `payload.tar.zst` file produced by
/// [`compress_payload`]. The manifest is serialised and the SHA-256
/// is written alongside.
pub fn write_bundle<W: Write>(
    out: W,
    manifest: &BundleManifest,
    payload_path: &Path,
) -> Result<(), BackupError> {
    let mut tar = tar::Builder::new(out);

    // 1. manifest.json
    let manifest_json = serde_json::to_vec_pretty(manifest)
        .map_err(|e| BackupError::Io(format!("serialize manifest: {e}")))?;
    append_bytes(&mut tar, "manifest.json", &manifest_json)?;

    // 2. payload.sha256
    let sha_content = format!("{}\n", manifest.payload_sha256);
    append_bytes(&mut tar, "payload.sha256", sha_content.as_bytes())?;

    // 3. payload.tar.zst
    let mut payload_file = std::fs::File::open(payload_path)
        .map_err(|e| BackupError::Io(format!("open payload: {e}")))?;
    let metadata = payload_file
        .metadata()
        .map_err(|e| BackupError::Io(format!("payload metadata: {e}")))?;

    let mut header = tar::Header::new_gnu();
    header.set_size(metadata.len());
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "payload.tar.zst", &mut payload_file)
        .map_err(|e| BackupError::Io(format!("tar payload: {e}")))?;

    tar.finish()
        .map_err(|e| BackupError::Io(format!("tar finish: {e}")))?;

    info!("bundle written");
    Ok(())
}

/// Helper: append raw bytes as a tar entry.
fn append_bytes<W: Write>(
    tar: &mut tar::Builder<W>,
    name: &str,
    data: &[u8],
) -> Result<(), BackupError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, name, data)
        .map_err(|e| BackupError::Io(format!("tar {name}: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// SHA-256 pass-through writer
// ---------------------------------------------------------------------------

/// A writer that hashes everything passing through while forwarding
/// to an inner `Write`.
struct Sha256HashWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Sha256HashWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn finalize_hex(self) -> String {
        hex::encode(self.hasher.finalize())
    }
}

impl<W: Write> Write for Sha256HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spi::backup::BundleManifest;
    use tempfile::TempDir;

    #[test]
    fn compress_and_read_back() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        std::fs::write(staging.join("test.txt"), b"hello backup").unwrap();

        let payload = tmp.path().join("payload.tar.zst");
        let sha = compress_payload(&staging, &payload).unwrap();
        assert_eq!(sha.len(), 64); // hex SHA-256

        // Decompress and verify contents.
        let compressed = std::fs::read(&payload).unwrap();
        let decompressed = zstd::decode_all(compressed.as_slice()).unwrap();
        let mut archive = tar::Archive::new(decompressed.as_slice());
        let entries: Vec<_> = archive
            .entries()
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn write_bundle_produces_valid_tar() {
        let tmp = TempDir::new().unwrap();
        let staging = tmp.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        std::fs::write(staging.join("data.bin"), b"snapshot data").unwrap();

        let payload_path = tmp.path().join("payload.tar.zst");
        let sha = compress_payload(&staging, &payload_path).unwrap();

        let manifest = BundleManifest::new_snapshot(
            "dev_test".into(),
            "0.42.1".into(),
            sha,
            1_735_689_600_000,
            "test@0.42.1".into(),
        );

        let bundle_path = tmp.path().join("test.listo-snapshot");
        let file = std::fs::File::create(&bundle_path).unwrap();
        write_bundle(file, &manifest, &payload_path).unwrap();

        // Read back and verify entries.
        let file = std::fs::File::open(&bundle_path).unwrap();
        let mut archive = tar::Archive::new(file);
        let names: Vec<String> = archive
            .entries()
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"payload.sha256".to_string()));
        assert!(names.contains(&"payload.tar.zst".to_string()));
    }
}
