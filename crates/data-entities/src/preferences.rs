//! Preference entity types — logical shape only.
//!
//! Physical DDL lives in `data-sqlite` (and future `data-postgres`).
//! Resolution (three-layer merge) lives in `data-repos::PreferencesService`.

use serde::{Deserialize, Serialize};

// ── System defaults ───────────────────────────────────────────────────────────

/// Hard-coded system defaults applied when neither user nor org has a value.
/// Explicit, not implicit — `GET /api/v1/me/preferences` always returns
/// concrete values, never `null` or `"auto"`.
pub const DEFAULT_TIMEZONE: &str = "UTC";
pub const DEFAULT_LOCALE: &str = "en-US";
pub const DEFAULT_LANGUAGE: &str = "en";
pub const DEFAULT_UNIT_SYSTEM: &str = "metric";
pub const DEFAULT_TEMPERATURE_UNIT: &str = "auto";
pub const DEFAULT_PRESSURE_UNIT: &str = "auto";
pub const DEFAULT_DATE_FORMAT: &str = "auto";
pub const DEFAULT_TIME_FORMAT: &str = "auto";
pub const DEFAULT_WEEK_START: &str = "auto";
pub const DEFAULT_NUMBER_FORMAT: &str = "auto";
pub const DEFAULT_CURRENCY: &str = "auto";
pub const DEFAULT_THEME: &str = "system";

// ── Storage types (nullable — one row per entity) ─────────────────────────────

/// Organisation-level preferences row.
/// `None` fields mean "not set at this layer"; resolved view fills them with
/// the next layer or the system default.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrgPreferences {
    pub org_id: String,

    pub timezone: Option<String>,
    pub locale: Option<String>,
    /// BCP-47 language subtag, e.g. `"en"`, `"zh"`. `None` → derived from
    /// `locale` by the resolver.
    pub language: Option<String>,
    /// `"metric"` | `"imperial"`. `None` → system default.
    pub unit_system: Option<String>,
    /// `"C"` | `"F"` | `"auto"`. `None` → `"auto"`.
    pub temperature_unit: Option<String>,
    /// `"kPa"` | `"psi"` | `"bar"` | `"auto"`. `None` → `"auto"`.
    pub pressure_unit: Option<String>,
    /// `"auto"` | `"YYYY-MM-DD"` | `"DD/MM/YYYY"` | `"MM/DD/YYYY"`.
    pub date_format: Option<String>,
    /// `"auto"` | `"24h"` | `"12h"`.
    pub time_format: Option<String>,
    /// `"auto"` | `"monday"` | `"sunday"`. Display-only, no canonical
    /// storage counterpart.
    pub week_start: Option<String>,
    /// `"auto"` | `"1,234.56"` | `"1.234,56"` | `"1 234,56"`.
    pub number_format: Option<String>,
    /// ISO 4217 code or `"auto"` (derive from locale). `None` → `"auto"`.
    pub currency: Option<String>,

    /// UTC epoch milliseconds of last update. `None` = never written.
    pub updated_at: Option<i64>,
}

/// Per-user-per-org preferences row. Same fields as [`OrgPreferences`] plus
/// `theme`. All nullable — `None` means "inherit from org layer".
///
/// A user has one row per org they belong to, keyed on `(user_id, org_id)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Stable user identifier (matches `Actor::User.id` as a UUID string).
    pub user_id: String,
    pub org_id: String,

    pub timezone: Option<String>,
    pub locale: Option<String>,
    pub language: Option<String>,
    pub unit_system: Option<String>,
    pub temperature_unit: Option<String>,
    pub pressure_unit: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub week_start: Option<String>,
    pub number_format: Option<String>,
    pub currency: Option<String>,

    /// `"light"` | `"dark"` | `"system"`. User-only — no org fallback.
    pub theme: Option<String>,

    pub updated_at: Option<i64>,
}

// ── Resolved (fully merged) view ──────────────────────────────────────────────

/// Fully resolved preferences: `user_value ?? org_value ?? system_default`.
///
/// All fields are non-optional. Clients never see `null` or `"auto"` in
/// this view — `"auto"` fields are left as the literal string `"auto"` here
/// because the *formatting interpretation* of `"auto"` is done client-side
/// (ICU4X derives from `locale`). The server resolves multi-layer
/// inheritance but does not expand `"auto"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPreferences {
    pub timezone: String,
    pub locale: String,
    pub language: String,
    pub unit_system: String,
    pub temperature_unit: String,
    pub pressure_unit: String,
    pub date_format: String,
    pub time_format: String,
    pub week_start: String,
    pub number_format: String,
    pub currency: String,
    /// Theme is user-only but included here for the `GET /me/preferences`
    /// resolved view so clients only need one call.
    pub theme: String,
}

impl Default for ResolvedPreferences {
    fn default() -> Self {
        Self {
            timezone: DEFAULT_TIMEZONE.to_string(),
            locale: DEFAULT_LOCALE.to_string(),
            language: DEFAULT_LANGUAGE.to_string(),
            unit_system: DEFAULT_UNIT_SYSTEM.to_string(),
            temperature_unit: DEFAULT_TEMPERATURE_UNIT.to_string(),
            pressure_unit: DEFAULT_PRESSURE_UNIT.to_string(),
            date_format: DEFAULT_DATE_FORMAT.to_string(),
            time_format: DEFAULT_TIME_FORMAT.to_string(),
            week_start: DEFAULT_WEEK_START.to_string(),
            number_format: DEFAULT_NUMBER_FORMAT.to_string(),
            currency: DEFAULT_CURRENCY.to_string(),
            theme: DEFAULT_THEME.to_string(),
        }
    }
}

// ── PATCH body (sparse update — only provided fields are changed) ─────────────

/// Patch payload for `PATCH /api/v1/me/preferences` and
/// `PATCH /api/v1/orgs/{id}/preferences`. Fields that are `None` in the
/// patch body are left unchanged.  Set a field to `Some(null)` in JSON to
/// revert it to "inherit from the layer below".
///
/// Since JSON `null` and `missing key` are different, we use
/// `serde_json::Value` for the outer Option to distinguish them.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PreferencesPatch {
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub timezone: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub locale: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub language: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub unit_system: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub temperature_unit: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub pressure_unit: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub date_format: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub time_format: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub week_start: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub number_format: MaybeUpdate,
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub currency: MaybeUpdate,
    /// Only meaningful for `PATCH /me/preferences`.
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub theme: MaybeUpdate,
}

/// Tri-state for a PATCH field:
/// - `Absent` — key not present; leave the stored value unchanged.
/// - `Clear`  — JSON `null`; revert to "inherit from layer below".
/// - `Set(v)` — new value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MaybeUpdate {
    #[default]
    Absent,
    Clear,
    Set(String),
}

fn de_nullable_string<'de, D>(de: D) -> Result<MaybeUpdate, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let opt = Option::<String>::deserialize(de)?;
    Ok(match opt {
        None => MaybeUpdate::Clear, // key present with explicit null
        Some(s) => MaybeUpdate::Set(s),
    })
}

impl MaybeUpdate {
    /// Apply this update on top of an existing `Option<String>` field.
    pub fn apply(self, current: Option<String>) -> Option<String> {
        match self {
            MaybeUpdate::Absent => current,
            MaybeUpdate::Clear => None,
            MaybeUpdate::Set(v) => Some(v),
        }
    }
}
