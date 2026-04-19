#![allow(clippy::unwrap_used, clippy::panic)]
//! Precedence + parsing behaviour of the config layers.

use std::path::PathBuf;

use config::{
    default_db_path, default_plugins_dir, from_file, AgentConfigOverlay, DatabaseOverlay,
    Defaults, LogOverlay, PluginsOverlay, Role,
};

fn defaults<'a>() -> Defaults<'a> {
    Defaults {
        db_path: &default_db_path,
        plugins_dir: &default_plugins_dir,
    }
}

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
        plugins: Some(PluginsOverlay {
            dir: Some(PathBuf::from("/from/file/plugins")),
        }),
    };
    let env = AgentConfigOverlay {
        role: Some(Role::Cloud),
        database: None,
        log: Some(LogOverlay {
            filter: Some("warn".into()),
        }),
        plugins: None,
    };
    let cli = AgentConfigOverlay {
        role: None,
        database: Some(DatabaseOverlay {
            path: Some(PathBuf::from("/from/cli.db")),
        }),
        log: None,
        plugins: Some(PluginsOverlay {
            dir: Some(PathBuf::from("/from/cli/plugins")),
        }),
    };

    // Stack: cli over env over file. `role`: only file + env set, env wins.
    // `database.path`: cli wins over file. `log.filter`: env wins over file.
    // `plugins.dir`: cli wins.
    let resolved = cli.merge_over(env).merge_over(file).resolve(defaults());

    assert_eq!(resolved.role, Role::Cloud);
    assert_eq!(resolved.database.path, Some(PathBuf::from("/from/cli.db")));
    assert_eq!(resolved.log.filter, "warn");
    assert_eq!(resolved.plugins.dir, PathBuf::from("/from/cli/plugins"));
}

#[test]
fn defaults_fill_in_when_nothing_specified() {
    let resolved = AgentConfigOverlay::default().resolve(defaults());
    assert_eq!(resolved.role, Role::Standalone);
    assert_eq!(resolved.database.path, Some(PathBuf::from("./agent.db")));
    assert_eq!(resolved.log.filter, "info");
    assert_eq!(resolved.plugins.dir, default_plugins_dir(Role::Standalone));
}

#[test]
fn cloud_role_has_no_default_db() {
    let overlay = AgentConfigOverlay {
        role: Some(Role::Cloud),
        ..Default::default()
    };
    let resolved = overlay.resolve(defaults());
    assert!(resolved.database.path.is_none());
}

#[test]
fn edge_role_defaults_plugins_under_var_lib() {
    let overlay = AgentConfigOverlay {
        role: Some(Role::Edge),
        ..Default::default()
    };
    let resolved = overlay.resolve(defaults());
    assert_eq!(resolved.plugins.dir, PathBuf::from("/var/lib/agent/plugins"));
}

#[test]
fn yaml_file_parses() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        "role: edge\ndatabase:\n  path: /srv/agent.db\nlog:\n  filter: debug\nplugins:\n  dir: /srv/plugins\n",
    )
    .unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    assert_eq!(overlay.role, Some(Role::Edge));
    assert_eq!(
        overlay.database.unwrap().path,
        Some(PathBuf::from("/srv/agent.db"))
    );
    assert_eq!(overlay.log.unwrap().filter, Some("debug".into()));
    assert_eq!(
        overlay.plugins.unwrap().dir,
        Some(PathBuf::from("/srv/plugins"))
    );
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
    assert!(file_layer.plugins.is_none());
}
