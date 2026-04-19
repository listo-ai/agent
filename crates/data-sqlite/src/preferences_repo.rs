//! SQLite implementation of [`PreferencesRepo`].

use std::path::Path;
use std::sync::Mutex;

use data_entities::{OrgPreferences, UserPreferences};
use data_repos::{PreferencesRepo, RepoError};
use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{open, Location};
use crate::error::SqliteError;

pub struct SqlitePreferencesRepo {
    conn: Mutex<Connection>,
}

impl SqlitePreferencesRepo {
    pub fn open_file(path: &Path) -> Result<Self, SqliteError> {
        Ok(Self {
            conn: Mutex::new(open(Location::File(path))?),
        })
    }

    pub fn open_memory() -> Result<Self, SqliteError> {
        Ok(Self {
            conn: Mutex::new(open(Location::InMemory)?),
        })
    }

    fn with_conn<R>(
        &self,
        f: impl FnOnce(&Connection) -> Result<R, SqliteError>,
    ) -> Result<R, RepoError> {
        let g = self
            .conn
            .lock()
            .map_err(|_| RepoError::Backend("sqlite preferences mutex poisoned".into()))?;
        Ok(f(&g)?)
    }
}

impl PreferencesRepo for SqlitePreferencesRepo {
    fn get_org(&self, org_id: &str) -> Result<Option<OrgPreferences>, RepoError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT org_id, timezone, locale, language, unit_system,
                         temperature_unit, pressure_unit, date_format, time_format,
                         week_start, number_format, currency, updated_at
                   FROM org_preferences
                  WHERE org_id = ?1",
                params![org_id],
                |row| {
                    Ok(OrgPreferences {
                        org_id: row.get(0)?,
                        timezone: row.get(1)?,
                        locale: row.get(2)?,
                        language: row.get(3)?,
                        unit_system: row.get(4)?,
                        temperature_unit: row.get(5)?,
                        pressure_unit: row.get(6)?,
                        date_format: row.get(7)?,
                        time_format: row.get(8)?,
                        week_start: row.get(9)?,
                        number_format: row.get(10)?,
                        currency: row.get(11)?,
                        updated_at: row.get(12)?,
                    })
                },
            )
            .optional()
            .map_err(SqliteError::from)
        })
    }

    fn set_org(&self, p: &OrgPreferences) -> Result<(), RepoError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO org_preferences
                    (org_id, timezone, locale, language, unit_system,
                     temperature_unit, pressure_unit, date_format, time_format,
                     week_start, number_format, currency, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(org_id) DO UPDATE SET
                     timezone         = excluded.timezone,
                     locale           = excluded.locale,
                     language         = excluded.language,
                     unit_system      = excluded.unit_system,
                     temperature_unit = excluded.temperature_unit,
                     pressure_unit    = excluded.pressure_unit,
                     date_format      = excluded.date_format,
                     time_format      = excluded.time_format,
                     week_start       = excluded.week_start,
                     number_format    = excluded.number_format,
                     currency         = excluded.currency,
                     updated_at       = excluded.updated_at",
                params![
                    p.org_id,
                    p.timezone,
                    p.locale,
                    p.language,
                    p.unit_system,
                    p.temperature_unit,
                    p.pressure_unit,
                    p.date_format,
                    p.time_format,
                    p.week_start,
                    p.number_format,
                    p.currency,
                    p.updated_at,
                ],
            )
            .map(|_| ())
            .map_err(SqliteError::from)
        })
    }

    fn get_user(&self, user_id: &str, org_id: &str) -> Result<Option<UserPreferences>, RepoError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT user_id, org_id, timezone, locale, language, unit_system,
                         temperature_unit, pressure_unit, date_format, time_format,
                         week_start, number_format, currency, theme, updated_at
                   FROM user_preferences
                  WHERE user_id = ?1 AND org_id = ?2",
                params![user_id, org_id],
                |row| {
                    Ok(UserPreferences {
                        user_id: row.get(0)?,
                        org_id: row.get(1)?,
                        timezone: row.get(2)?,
                        locale: row.get(3)?,
                        language: row.get(4)?,
                        unit_system: row.get(5)?,
                        temperature_unit: row.get(6)?,
                        pressure_unit: row.get(7)?,
                        date_format: row.get(8)?,
                        time_format: row.get(9)?,
                        week_start: row.get(10)?,
                        number_format: row.get(11)?,
                        currency: row.get(12)?,
                        theme: row.get(13)?,
                        updated_at: row.get(14)?,
                    })
                },
            )
            .optional()
            .map_err(SqliteError::from)
        })
    }

    fn set_user(&self, p: &UserPreferences) -> Result<(), RepoError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO user_preferences
                    (user_id, org_id, timezone, locale, language, unit_system,
                     temperature_unit, pressure_unit, date_format, time_format,
                     week_start, number_format, currency, theme, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
                 ON CONFLICT(user_id, org_id) DO UPDATE SET
                     timezone         = excluded.timezone,
                     locale           = excluded.locale,
                     language         = excluded.language,
                     unit_system      = excluded.unit_system,
                     temperature_unit = excluded.temperature_unit,
                     pressure_unit    = excluded.pressure_unit,
                     date_format      = excluded.date_format,
                     time_format      = excluded.time_format,
                     week_start       = excluded.week_start,
                     number_format    = excluded.number_format,
                     currency         = excluded.currency,
                     theme            = excluded.theme,
                     updated_at       = excluded.updated_at",
                params![
                    p.user_id,
                    p.org_id,
                    p.timezone,
                    p.locale,
                    p.language,
                    p.unit_system,
                    p.temperature_unit,
                    p.pressure_unit,
                    p.date_format,
                    p.time_format,
                    p.week_start,
                    p.number_format,
                    p.currency,
                    p.theme,
                    p.updated_at,
                ],
            )
            .map(|_| ())
            .map_err(SqliteError::from)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> SqlitePreferencesRepo {
        SqlitePreferencesRepo::open_memory().unwrap()
    }

    #[test]
    fn org_round_trip() {
        let repo = open();
        assert!(repo.get_org("o1").unwrap().is_none());

        let p = OrgPreferences {
            org_id: "o1".to_string(),
            timezone: Some("Australia/Brisbane".to_string()),
            locale: Some("en-AU".to_string()),
            ..Default::default()
        };
        repo.set_org(&p).unwrap();

        let back = repo.get_org("o1").unwrap().unwrap();
        assert_eq!(back.timezone.as_deref(), Some("Australia/Brisbane"));
        assert_eq!(back.locale.as_deref(), Some("en-AU"));
        assert!(back.language.is_none());
    }

    #[test]
    fn org_upsert_updates_existing() {
        let repo = open();
        let p = OrgPreferences {
            org_id: "o1".to_string(),
            timezone: Some("UTC".to_string()),
            ..Default::default()
        };
        repo.set_org(&p).unwrap();

        let p2 = OrgPreferences {
            org_id: "o1".to_string(),
            timezone: Some("America/New_York".to_string()),
            locale: Some("en-US".to_string()),
            ..Default::default()
        };
        repo.set_org(&p2).unwrap();

        let back = repo.get_org("o1").unwrap().unwrap();
        assert_eq!(back.timezone.as_deref(), Some("America/New_York"));
        assert_eq!(back.locale.as_deref(), Some("en-US"));
    }

    #[test]
    fn user_round_trip() {
        let repo = open();
        assert!(repo.get_user("u1", "o1").unwrap().is_none());

        let p = UserPreferences {
            user_id: "u1".to_string(),
            org_id: "o1".to_string(),
            timezone: Some("Europe/London".to_string()),
            theme: Some("dark".to_string()),
            ..Default::default()
        };
        repo.set_user(&p).unwrap();

        let back = repo.get_user("u1", "o1").unwrap().unwrap();
        assert_eq!(back.timezone.as_deref(), Some("Europe/London"));
        assert_eq!(back.theme.as_deref(), Some("dark"));
        assert!(back.locale.is_none());
    }

    #[test]
    fn user_clear_field_stores_null() {
        let repo = open();
        let p = UserPreferences {
            user_id: "u1".to_string(),
            org_id: "o1".to_string(),
            timezone: Some("UTC".to_string()),
            ..Default::default()
        };
        repo.set_user(&p).unwrap();

        // Simulate a PATCH Clear by writing None.
        let p2 = UserPreferences {
            user_id: "u1".to_string(),
            org_id: "o1".to_string(),
            timezone: None,
            ..Default::default()
        };
        repo.set_user(&p2).unwrap();

        let back = repo.get_user("u1", "o1").unwrap().unwrap();
        assert!(back.timezone.is_none());
    }

    #[test]
    fn users_are_scoped_per_org() {
        let repo = open();
        let p1 = UserPreferences {
            user_id: "u1".to_string(),
            org_id: "org-a".to_string(),
            timezone: Some("UTC".to_string()),
            ..Default::default()
        };
        let p2 = UserPreferences {
            user_id: "u1".to_string(),
            org_id: "org-b".to_string(),
            timezone: Some("America/New_York".to_string()),
            ..Default::default()
        };
        repo.set_user(&p1).unwrap();
        repo.set_user(&p2).unwrap();

        assert_eq!(
            repo.get_user("u1", "org-a")
                .unwrap()
                .unwrap()
                .timezone
                .as_deref(),
            Some("UTC")
        );
        assert_eq!(
            repo.get_user("u1", "org-b")
                .unwrap()
                .unwrap()
                .timezone
                .as_deref(),
            Some("America/New_York")
        );
    }
}
