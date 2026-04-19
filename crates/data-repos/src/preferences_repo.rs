//! `PreferencesRepo` trait + `PreferencesService` (three-layer resolution).
//!
//! Implementations live in `data-sqlite` (and future `data-postgres`).
//! The service separates the resolution algorithm from storage so the
//! algorithm is testable without a database.

use std::sync::Arc;

use data_entities::{
    MaybeUpdate, OrgPreferences, PreferencesPatch, ResolvedPreferences,
    UserPreferences,
};

use crate::RepoError;

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Storage contract for user and org preferences.
///
/// All methods are synchronous — SQLite is naturally sync and the
/// hot path (every request) must not pay async overhead for a simple
/// row lookup. A future `data-postgres` impl can wrap blocking calls
/// in `tokio::task::spawn_blocking`.
pub trait PreferencesRepo: Send + Sync + 'static {
    /// Return the org-layer preferences row for `org_id`, or `None` if
    /// the org has never written any preferences.
    fn get_org(&self, org_id: &str) -> Result<Option<OrgPreferences>, RepoError>;

    /// Upsert the org-layer preferences row.
    fn set_org(&self, prefs: &OrgPreferences) -> Result<(), RepoError>;

    /// Return the user-per-org preferences row for `(user_id, org_id)`,
    /// or `None` if the user has never written any preferences for that org.
    fn get_user(
        &self,
        user_id: &str,
        org_id: &str,
    ) -> Result<Option<UserPreferences>, RepoError>;

    /// Upsert the user-per-org preferences row.
    fn set_user(&self, prefs: &UserPreferences) -> Result<(), RepoError>;
}

// ── Service ───────────────────────────────────────────────────────────────────

/// High-level service that owns the three-layer resolution logic and
/// applies `PreferencesPatch` updates cleanly.
///
/// Clone-cheap: wraps an `Arc<dyn PreferencesRepo>`.
#[derive(Clone)]
pub struct PreferencesService {
    repo: Arc<dyn PreferencesRepo>,
}

impl PreferencesService {
    pub fn new(repo: Arc<dyn PreferencesRepo>) -> Self {
        Self { repo }
    }

    // ── Reads ─────────────────────────────────────────────────────────────────

    /// Return the fully resolved preferences for `(user_id, org_id)`.
    ///
    /// Resolution: `user_value ?? org_value ?? system_default`.
    pub fn resolved(
        &self,
        user_id: &str,
        org_id: &str,
    ) -> Result<ResolvedPreferences, RepoError> {
        let user = self.repo.get_user(user_id, org_id)?;
        let org = self.repo.get_org(org_id)?;
        Ok(resolve(user.as_ref(), org.as_ref()))
    }

    /// Return the raw user-layer row (all fields nullable).
    pub fn user_layer(
        &self,
        user_id: &str,
        org_id: &str,
    ) -> Result<UserPreferences, RepoError> {
        Ok(self
            .repo
            .get_user(user_id, org_id)?
            .unwrap_or_else(|| UserPreferences {
                user_id: user_id.to_string(),
                org_id: org_id.to_string(),
                ..Default::default()
            }))
    }

    /// Return the raw org-layer row (all fields nullable).
    pub fn org_layer(&self, org_id: &str) -> Result<OrgPreferences, RepoError> {
        Ok(self
            .repo
            .get_org(org_id)?
            .unwrap_or_else(|| OrgPreferences {
                org_id: org_id.to_string(),
                ..Default::default()
            }))
    }

    // ── Writes ────────────────────────────────────────────────────────────────

    /// Apply a sparse patch to the user layer for `(user_id, org_id)`.
    ///
    /// `Absent` fields are left unchanged. `Clear` fields revert to
    /// "inherit from org". `Set(v)` fields are written.
    pub fn patch_user(
        &self,
        user_id: &str,
        org_id: &str,
        patch: PreferencesPatch,
        now_ms: i64,
    ) -> Result<ResolvedPreferences, RepoError> {
        let mut row = self
            .repo
            .get_user(user_id, org_id)?
            .unwrap_or_else(|| UserPreferences {
                user_id: user_id.to_string(),
                org_id: org_id.to_string(),
                ..Default::default()
            });

        row.timezone = patch.timezone.apply(row.timezone);
        row.locale = patch.locale.apply(row.locale);
        row.language = patch.language.apply(row.language);
        row.unit_system = patch.unit_system.apply(row.unit_system);
        row.temperature_unit = patch.temperature_unit.apply(row.temperature_unit);
        row.pressure_unit = patch.pressure_unit.apply(row.pressure_unit);
        row.date_format = patch.date_format.apply(row.date_format);
        row.time_format = patch.time_format.apply(row.time_format);
        row.week_start = patch.week_start.apply(row.week_start);
        row.number_format = patch.number_format.apply(row.number_format);
        row.currency = patch.currency.apply(row.currency);

        // Theme only applies to the user layer (no org fallback).
        if patch.theme != MaybeUpdate::Absent {
            row.theme = patch.theme.apply(row.theme);
        }

        row.updated_at = Some(now_ms);
        self.repo.set_user(&row)?;

        // Return the fully resolved view after the write.
        self.resolved(user_id, org_id)
    }

    /// Apply a sparse patch to the org layer.
    pub fn patch_org(
        &self,
        org_id: &str,
        patch: PreferencesPatch,
        now_ms: i64,
    ) -> Result<OrgPreferences, RepoError> {
        let mut row = self
            .repo
            .get_org(org_id)?
            .unwrap_or_else(|| OrgPreferences {
                org_id: org_id.to_string(),
                ..Default::default()
            });

        row.timezone = patch.timezone.apply(row.timezone);
        row.locale = patch.locale.apply(row.locale);
        row.language = patch.language.apply(row.language);
        row.unit_system = patch.unit_system.apply(row.unit_system);
        row.temperature_unit = patch.temperature_unit.apply(row.temperature_unit);
        row.pressure_unit = patch.pressure_unit.apply(row.pressure_unit);
        row.date_format = patch.date_format.apply(row.date_format);
        row.time_format = patch.time_format.apply(row.time_format);
        row.week_start = patch.week_start.apply(row.week_start);
        row.number_format = patch.number_format.apply(row.number_format);
        row.currency = patch.currency.apply(row.currency);
        // `theme` is user-only; silently ignore if included in an org patch.

        row.updated_at = Some(now_ms);
        self.repo.set_org(&row)?;
        Ok(row)
    }
}

// ── Resolution algorithm ──────────────────────────────────────────────────────

/// Three-layer null-coalesce: `user ?? org ?? system_default`.
fn resolve(
    user: Option<&UserPreferences>,
    org: Option<&OrgPreferences>,
) -> ResolvedPreferences {
    let defaults = ResolvedPreferences::default();

    macro_rules! pick {
        ($field:ident) => {
            user.and_then(|u| u.$field.clone())
                .or_else(|| org.and_then(|o| o.$field.clone()))
                .unwrap_or_else(|| defaults.$field.clone())
        };
    }

    // `theme` is not on `OrgPreferences`, so it skips the org layer.
    let theme = user
        .and_then(|u| u.theme.clone())
        .unwrap_or_else(|| defaults.theme.clone());

    ResolvedPreferences {
        timezone: pick!(timezone),
        locale: pick!(locale),
        language: pick!(language),
        unit_system: pick!(unit_system),
        temperature_unit: pick!(temperature_unit),
        pressure_unit: pick!(pressure_unit),
        date_format: pick!(date_format),
        time_format: pick!(time_format),
        week_start: pick!(week_start),
        number_format: pick!(number_format),
        currency: pick!(currency),
        theme,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use data_entities::{
        DEFAULT_LANGUAGE, DEFAULT_LOCALE, DEFAULT_THEME, DEFAULT_TIMEZONE,
        DEFAULT_UNIT_SYSTEM,
    };

    fn user(timezone: Option<&str>, locale: Option<&str>) -> UserPreferences {
        UserPreferences {
            user_id: "u1".to_string(),
            org_id: "o1".to_string(),
            timezone: timezone.map(str::to_string),
            locale: locale.map(str::to_string),
            ..Default::default()
        }
    }

    fn org(timezone: Option<&str>, locale: Option<&str>) -> OrgPreferences {
        OrgPreferences {
            org_id: "o1".to_string(),
            timezone: timezone.map(str::to_string),
            locale: locale.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn system_defaults_when_no_rows() {
        let resolved = resolve(None, None);
        assert_eq!(resolved.timezone, DEFAULT_TIMEZONE);
        assert_eq!(resolved.locale, DEFAULT_LOCALE);
        assert_eq!(resolved.unit_system, DEFAULT_UNIT_SYSTEM);
        assert_eq!(resolved.theme, DEFAULT_THEME);
    }

    #[test]
    fn org_overrides_defaults() {
        let resolved = resolve(None, Some(&org(Some("Australia/Brisbane"), None)));
        assert_eq!(resolved.timezone, "Australia/Brisbane");
        assert_eq!(resolved.locale, DEFAULT_LOCALE);
    }

    #[test]
    fn user_overrides_org() {
        let resolved = resolve(
            Some(&user(Some("America/New_York"), None)),
            Some(&org(Some("Australia/Brisbane"), Some("en-AU"))),
        );
        assert_eq!(resolved.timezone, "America/New_York");
        // user has no locale → falls back to org
        assert_eq!(resolved.locale, "en-AU");
    }

    #[test]
    fn theme_uses_system_default_when_absent_on_user() {
        let resolved = resolve(Some(&user(None, None)), None);
        assert_eq!(resolved.theme, DEFAULT_THEME);
    }

    #[test]
    fn maybe_update_absent_preserves_existing() {
        assert_eq!(
            MaybeUpdate::Absent.apply(Some("UTC".to_string())),
            Some("UTC".to_string())
        );
    }

    #[test]
    fn maybe_update_clear_removes_value() {
        assert_eq!(MaybeUpdate::Clear.apply(Some("UTC".to_string())), None);
    }

    #[test]
    fn language_falls_back_through_layers() {
        // User has explicit language; org has different language.
        let u = UserPreferences {
            language: Some("zh".to_string()),
            ..user(None, None)
        };
        let o = org(None, None);
        let resolved = resolve(Some(&u), Some(&o));
        assert_eq!(resolved.language, "zh");
    }

    #[test]
    fn language_falls_back_to_org() {
        let u = user(None, None); // no language on user
        let mut o = org(None, None);
        o.language = Some("es".to_string());
        let resolved = resolve(Some(&u), Some(&o));
        assert_eq!(resolved.language, "es");
    }

    #[test]
    fn language_falls_back_to_system() {
        let resolved = resolve(Some(&user(None, None)), Some(&org(None, None)));
        assert_eq!(resolved.language, DEFAULT_LANGUAGE);
    }
}
