#![allow(clippy::unwrap_used, clippy::panic)]
//! Precedence + parsing behaviour of the config layers.

use std::path::PathBuf;

use config::{
    default_blocks_dir, default_db_path, from_file, AgentConfigOverlay, DatabaseOverlay, Defaults,
    LogOverlay, PluginsOverlay, Role,
};

fn defaults<'a>() -> Defaults<'a> {
    Defaults {
        db_path: &default_db_path,
        blocks_dir: &default_blocks_dir,
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
        blocks: Some(PluginsOverlay {
            dir: Some(PathBuf::from("/from/file/blocks")),
        }),
        fleet: None,
        auth: None,
    };
    let env = AgentConfigOverlay {
        role: Some(Role::Cloud),
        database: None,
        log: Some(LogOverlay {
            filter: Some("warn".into()),
        }),
        blocks: None,
        fleet: None,
        auth: None,
    };
    let cli = AgentConfigOverlay {
        role: None,
        database: Some(DatabaseOverlay {
            path: Some(PathBuf::from("/from/cli.db")),
        }),
        log: None,
        blocks: Some(PluginsOverlay {
            dir: Some(PathBuf::from("/from/cli/blocks")),
        }),
        fleet: None,
        auth: None,
    };

    // Stack: cli over env over file. `role`: only file + env set, env wins.
    // `database.path`: cli wins over file. `log.filter`: env wins over file.
    // `blocks.dir`: cli wins.
    let resolved = cli.merge_over(env).merge_over(file).resolve(defaults());

    assert_eq!(resolved.role, Role::Cloud);
    assert_eq!(resolved.database.path, Some(PathBuf::from("/from/cli.db")));
    assert_eq!(resolved.log.filter, "warn");
    assert_eq!(resolved.blocks.dir, PathBuf::from("/from/cli/blocks"));
}

#[test]
fn defaults_fill_in_when_nothing_specified() {
    let resolved = AgentConfigOverlay::default().resolve(defaults());
    assert_eq!(resolved.role, Role::Standalone);
    assert_eq!(resolved.database.path, Some(PathBuf::from("./agent.db")));
    assert_eq!(resolved.log.filter, "info");
    assert_eq!(resolved.blocks.dir, default_blocks_dir(Role::Standalone));
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
    assert_eq!(resolved.blocks.dir, PathBuf::from("/var/lib/agent/blocks"));
}

#[test]
fn yaml_file_parses() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        "role: edge\ndatabase:\n  path: /srv/agent.db\nlog:\n  filter: debug\nplugins:\n  dir: /srv/blocks\n",
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
        overlay.blocks.unwrap().dir,
        Some(PathBuf::from("/srv/blocks"))
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
fn fleet_zenoh_yaml_parses_and_resolves() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        concat!(
            "role: standalone\n",
            "fleet:\n",
            "  backend: zenoh\n",
            "  listen: [\"tcp/0.0.0.0:7447\"]\n",
            "  connect: []\n",
            "  tenant: sys\n",
            "  agent_id: edge-1\n",
        ),
    )
    .unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    let resolved = overlay.resolve(defaults());
    match resolved.fleet {
        config::FleetConfig::Zenoh {
            listen,
            connect,
            tenant,
            agent_id,
        } => {
            assert_eq!(listen, vec!["tcp/0.0.0.0:7447"]);
            assert!(connect.is_empty());
            assert_eq!(tenant, "sys");
            assert_eq!(agent_id, "edge-1");
        }
        other => panic!("expected zenoh, got {other:?}"),
    }
}

#[test]
fn auth_static_token_yaml_parses_and_resolves() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        concat!(
            "role: cloud\n",
            "auth:\n",
            "  provider: static_token\n",
            "  tokens:\n",
            "    - token: \"ed_sys_edge1_xxxxx\"\n",
            "      actor:\n",
            "        kind: machine\n",
            "        id: \"00000000-0000-0000-0000-000000000001\"\n",
            "        label: \"edge-1\"\n",
            "      tenant: sys\n",
            "      scopes: [read_nodes, write_slots, manage_fleet]\n",
        ),
    )
    .unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    let resolved = overlay.resolve(defaults());
    match resolved.auth {
        config::AuthConfig::StaticToken { tokens } => {
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].token, "ed_sys_edge1_xxxxx");
            assert_eq!(tokens[0].tenant.as_str(), "sys");
            assert_eq!(tokens[0].scopes.len(), 3);
        }
        other => panic!("expected StaticToken, got {other:?}"),
    }
}

#[test]
fn auth_absent_yaml_resolves_to_dev_null() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "role: standalone\n").unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    let resolved = overlay.resolve(defaults());
    assert_eq!(resolved.auth, config::AuthConfig::DevNull);
}

#[test]
fn fleet_absent_yaml_resolves_to_null() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "role: edge\n").unwrap();
    let overlay = from_file(tmp.path()).unwrap();
    let resolved = overlay.resolve(defaults());
    assert_eq!(resolved.fleet, config::FleetConfig::Null);
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
    assert!(file_layer.blocks.is_none());
}
