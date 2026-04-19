//! Shared entity structs + column enums.
//!
//! Logical shape only — physical DDL diverges per backend (see
//! `data-sqlite` / `data-postgres`). Domain crates depend on these
//! types via the repo traits in `data-repos`.

pub mod ids;
pub mod preferences;
pub mod revisions;

pub use ids::{DeviceId, FlowId, NodeId, RevisionId};
pub use preferences::{
    MaybeUpdate, OrgPreferences, PreferencesPatch, ResolvedPreferences, UserPreferences,
    DEFAULT_CURRENCY, DEFAULT_DATE_FORMAT, DEFAULT_LANGUAGE, DEFAULT_LOCALE, DEFAULT_NUMBER_FORMAT,
    DEFAULT_PRESSURE_UNIT, DEFAULT_TEMPERATURE_UNIT, DEFAULT_THEME, DEFAULT_TIMEZONE,
    DEFAULT_TIME_FORMAT, DEFAULT_UNIT_SYSTEM, DEFAULT_WEEK_START,
};
pub use revisions::{FlowDocument, FlowRevision, RevisionOp};
