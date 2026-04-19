#![allow(clippy::unwrap_used, clippy::panic)]
//! Precedence + parsing behaviour of the config layers.

use std::path::PathBuf;

use config::{default_db_path, from_file, AgentConfigOverlay, DatabaseOverlay, LogOverlay, Role};

#[test]
fn cli_over_env_over_file_over_defaults() {
    let file = AgentConfigOverlay {
        role: Some(Role::Edge),
        database: Some(DatabaseOverlay {
            path: Some(PathBuf::from("/from/file.db")),
        }),
        log: Some(LogOverlay {
            filter: Some("debug".into()),
        }),
    };
    let env = AgentConfigOverlay {
        role: Some(Role::Cloud),
        database: None,
        log: Some(LogOverlay {
            filter: Some("warn".into()),
        }),
    };
    let cli = AgentConfigOverlay {
        role: None,
        database: Some(DatabaseOverlay {
            path: Some(PathBuf::from("/from/cli.db")),
        }),
        log: None,
    };

    // Stack: cli over env over file. `role`: only file + env set, env wins.
    // `database.path`: cli wins over file. `log.filter`: env wins over file.
    let resolved = cli
        .merge_over(env)
        .merge_over(file)
        .resolve(default_db_path);

    assert_eq!(resolved.role, Role::Cloud);
    assert_eq!(resolved.database.path, Some(PathBuf::from("/from/cli.db")));
    assert_eq!(resolved.log.filter, "warn");
}

#[test]
fn defaults_fill_in_when_nothing_specified() {
    let resolved = AgentConfigOverlay::default().resolve(default_db_path);
    assert_eq!(resolved.role, Role::Standalone);
    assert_eq!(resolved.database.path, Some(PathBuf::from("./agent.db")));
    assert_eq!(resolved.log.filter, "info");
}

#[test]
fn cloud_role_has_no_default_db() {
    let overlay = AgentConfigOverlay {
        role: Some(Role::Cloud),
        ..Default::default()
    };
    let resolved = overlay.resolve(default_db_path);
    assert!(resolved.database.path.is_none());
}

#[test]
fn yaml_file_parses() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        "role: edge\ndatabase:\n  path: /srv/agent.db\nlog:\n  filter: debug\n",
    )
    .unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    assert_eq!(overlay.role, Some(Role::Edge));
    assert_eq!(
        overlay.database.unwrap().path,
        Some(PathBuf::from("/srv/agent.db"))
    );
    assert_eq!(overlay.log.unwrap().filter, Some("debug".into()));
}

#[test]
fn unknown_yaml_field_rejected() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "role: edge\nmystery_field: 42\n").unwrap();
    let err = from_file(tmp.path()).unwrap_err();
    assert!(format!("{err}").contains("mystery_field"));
}

#[test]
fn partial_yaml_leaves_other_fields_for_later_layers() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    // Only role is set. `database` and `log` default to None in the overlay
    // \u{2014} a later layer gets to own them.
    std::fs::write(tmp.path(), "role: cloud\n").unwrap();
    let file_layer = from_file(tmp.path()).unwrap();
    assert_eq!(file_layer.role, Some(Role::Cloud));
    assert!(file_layer.database.is_none());
    assert!(file_layer.log.is_none());
}
