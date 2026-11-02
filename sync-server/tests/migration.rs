//! Migration 3 → 4 (HITOP Cloud organizations) of a T25-shaped SQLite database, as the test
//! server has it: see common/t25.rs. The same check on Postgres is in tests/postgres.rs.

mod common;

use common::{t25, Server};
use hitop_sync_server::Db;

#[test]
fn a_t25_database_migrates_to_orgs_with_grandfathered_limits() {
    let dir = std::env::temp_dir().join(format!("hitop-sync-t25-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite:{}", dir.join("sync.db").display());
    t25::build(&url);
    let before = t25::counts(&Db::open_at(&url, 3).unwrap());
    let s = Server::with("t25-migration", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    let version = s.app.state().db.tx(|tx| tx.query_one("SELECT MAX(version) FROM schema_version", &[])).unwrap().unwrap().int(0);
    assert_eq!(version as usize, Db::schema_version());
    t25::check(&s, &before);
    drop(s);
    // Opening again migrates nothing twice.
    Db::open(&url).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_new_database_has_no_orgs_until_people_sign_in() {
    let s = Server::new("t25-fresh");
    let n = s.app.state().db.tx(|tx| tx.query_one("SELECT COUNT(*) FROM orgs", &[])).unwrap().unwrap().int(0);
    assert_eq!(n, 0);
    let d = common::Device::new(&s, "new@example.com");
    let orgs = s.call("GET", "/v1/orgs", Some(&d.session), None);
    assert_eq!(orgs.json["orgs"].as_array().unwrap().len(), 1, "a personal org on first use");
    assert_eq!(orgs.json["orgs"][0]["id"], format!("personal-{}", d.user_id));
}
