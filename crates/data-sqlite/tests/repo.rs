#![allow(clippy::unwrap_used, clippy::panic)]
//! Runs the shared `GraphRepo` trait-test harness against
//! `SqliteGraphRepo`. Any regression caught here is at the seam, not in
//! a downstream consumer.

use data_repos::testing::run_all;
use data_sqlite::SqliteGraphRepo;

#[test]
fn sqlite_satisfies_graph_repo_contract() {
    run_all(|| SqliteGraphRepo::open_memory().expect("open memory sqlite"));
}

#[test]
fn file_backed_survives_reopen() {
    use data_repos::{GraphRepo, PersistedNode};
    use uuid::Uuid;

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    let id = Uuid::new_v4();

    {
        let repo = SqliteGraphRepo::open_file(&path).unwrap();
        repo.save_node(&PersistedNode {
            id,
            parent_id: None,
            kind_id: "acme.core.station".into(),
            path: "/".into(),
            name: "/".into(),
            lifecycle: "active".into(),
        })
        .unwrap();
    }

    let repo = SqliteGraphRepo::open_file(&path).unwrap();
    let snap = repo.load().unwrap();
    assert_eq!(snap.nodes.len(), 1);
    assert_eq!(snap.nodes[0].id, id);
    assert_eq!(snap.nodes[0].lifecycle, "active");
}
