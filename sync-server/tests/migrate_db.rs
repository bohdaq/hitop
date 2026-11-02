//! `hitop-sync-server migrate-db`, `db-stats` and `snapshot` on SQLite (the Postgres switch and its
//! rollback are in tests/postgres.rs): every row arrives, the content hashes match, the audit
//! chains verify on the copy, and a target that isn't empty is refused.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::{fill, Server};
use hitop_sync_server::{dbtools, Db};

fn bin(args: &[&str], database: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hitop-sync-server"));
    c.args(args).env_remove("HITOP_SYNC_DATABASE");
    if let Some(db) = database {
        c.env("HITOP_SYNC_DATABASE", db);
    }
    c.output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn the_table_list_is_every_table_in_the_schema() {
    let s = Server::new("tables");
    let mut in_db: Vec<String> = s
        .app
        .state()
        .db
        .tx(|tx| tx.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name <> 'schema_version' ORDER BY name", &[]))
        .unwrap()
        .iter()
        .map(|r| r.text(0))
        .collect();
    let mut listed = Db::tables();
    listed.sort();
    in_db.sort();
    assert_eq!(listed, in_db);
}

#[test]
fn sqlite_to_sqlite_copies_every_row_and_verifies_the_audit_chains() {
    let s = Server::new("migrate-src");
    let chains = fill::fill(&s);
    assert!(chains >= 2, "accounts and the workspace have chains");
    let src = format!("sqlite:{}", s.dir.join("sync.db").display());
    let out_dir = s.dir.join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    let dst = format!("sqlite:{}", out_dir.join("copy.db").display());

    let before = dbtools::stats(&s.app.state().db).unwrap();
    for t in ["users", "device_keys", "workspaces", "members", "wraps", "blobs", "invites", "audit_log", "audit_chain", "orgs", "org_members", "entitlements", "api_tokens", "sessions"] {
        assert!(before.tables.iter().any(|x| x.table == t && x.rows > 0), "{t} has rows to copy");
    }
    let o = bin(&["migrate-db", "--from", &src, "--to", &dst], None);
    let out = text(&o);
    assert_eq!(o.status.code(), Some(0), "{out}");
    assert!(out.contains("row counts and content hashes equal on both sides"), "{out}");
    assert!(out.contains(&format!("{chains} chains, 0 broken")), "{out}");

    let copy = Db::open(&dst).unwrap();
    assert_eq!(dbtools::stats(&copy).unwrap().tables, before.tables);
    drop(copy);

    // db-stats prints the same numbers for both, as JSON too.
    let a: serde_json::Value = serde_json::from_slice(&bin(&["db-stats", "--json"], Some(&src)).stdout).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&bin(&["db-stats", "--json"], Some(&dst)).stdout).unwrap();
    assert_eq!(a["tables"], b["tables"]);
    assert_eq!(a["schema"], Db::schema_version() as i64);

    // Never into a database that has something in it, never over an existing file.
    let o = bin(&["migrate-db", "--from", &src, "--to", &dst], None);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("exists"), "{}", text(&o));
    let other = Server::new("migrate-nonempty");
    common::Device::new(&other, "someone@example.com");
    let err = dbtools::copy(&s.app.state().db, &other.app.state().db).unwrap_err();
    assert!(err.0.contains("not empty"), "{err}");
    // Usage errors.
    assert_eq!(bin(&["migrate-db", "--from", &src], None).status.code(), Some(2));
    assert_eq!(bin(&["migrate-db", "--from", &src, "--to", &src], None).status.code(), Some(2));
    let missing = format!("sqlite:{}", s.dir.join("nope.db").display());
    assert_eq!(bin(&["migrate-db", "--from", &missing, "--to", &format!("sqlite:{}", out_dir.join("x.db").display())], None).status.code(), Some(2));
}

#[test]
fn a_changed_row_changes_the_table_hash() {
    let s = Server::new("migrate-hash");
    fill::fill(&s);
    let db = &s.app.state().db;
    let before = dbtools::stats(db).unwrap();
    db.tx(|tx| tx.execute("UPDATE blobs SET content = $1 WHERE id = 'b'", &[vec![1u8, 2, 3].into()])).unwrap();
    let after = dbtools::stats(db).unwrap();
    let changed: Vec<&str> = before.tables.iter().zip(&after.tables).filter(|(a, b)| a != b).map(|(a, _)| a.table.as_str()).collect();
    assert_eq!(changed, ["blobs"]);
}

#[test]
fn snapshot_is_a_consistent_copy_that_verifies() {
    let s = Server::new("snapshot");
    let chains = fill::fill(&s);
    let src = format!("sqlite:{}", s.dir.join("sync.db").display());
    let out = s.dir.join("snap.db");
    let o = bin(&["snapshot", out.to_str().unwrap()], Some(&src));
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(Path::new(&out).exists());
    let snap = format!("sqlite:{}", out.display());
    let o = bin(&["audit-verify"], Some(&snap));
    assert!(text(&o).contains(&format!("{chains} chains, 0 broken")), "{}", text(&o));
    assert_eq!(dbtools::stats(&Db::open(&snap).unwrap()).unwrap().tables, dbtools::stats(&s.app.state().db).unwrap().tables);
    // Never over an existing file.
    assert_eq!(bin(&["snapshot", out.to_str().unwrap()], Some(&src)).status.code(), Some(2));
}
