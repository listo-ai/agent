// STAGE-1 complete — domain-backup: export + restore + sys.backup.config

#![forbid(unsafe_code)]
//! `domain-backup` — backup & restore orchestration.
//!
//! Pure logic crate: export/import, template vs snapshot, conflict
//! resolver, portability filter, device_id check. **No HTTP, no SQL.**
//! See `agent/docs/design/BACKUP.md` and `SKILLS/CODE-LAYOUT.md`.
//!
//! ## What lives here
//!
//! * [`config`] — the `sys.backup.config` node kind + settings schema.
//! * [`export`] — snapshot export orchestration.
//! * [`restore`] — snapshot restore plan + verification.
//! * [`error`] — error types.
//! * (Phase 2) `template` — template export/import + conflict resolver.

pub mod config;
pub mod error;
pub mod export;
pub mod restore;
pub mod template;

pub use config::BackupConfigSettings;
pub use error::{ExportError, RestoreError};
pub use export::{export_snapshot, SnapshotExportInput, SnapshotExportResult};
pub use restore::{prepare_restore, RestorePlan};
pub use template::{
    export_template, plan_import, read_template_doc, ConflictStrategy, ImportPlan,
    TemplateDoc, TemplateExportInput, TemplateExportResult, TemplateNode,
};

/// Register every kind manifest this crate contributes.
pub fn register_kinds(kinds: &graph::KindRegistry) {
    kinds.register(config::manifest());
}
