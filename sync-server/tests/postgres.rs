//! The same flows on Postgres (feature `postgres`). Runs when HITOP_SYNC_TEST_POSTGRES points at
//! a server (CI starts one), each test in a schema of its own; skipped otherwise.
#![cfg(feature = "postgres")]

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "00112233445566778899aabbccddeeff";

fn database(name: &str) -> Option<String> {
    let url = std::env::var("HITOP_SYNC_TEST_POSTGRES").ok()?;
    let schema = format!("t_{name}_{}", std::process::id());
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).expect("HITOP_SYNC_TEST_POSTGRES");
    admin.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}")).unwrap();
    let sep = if url.contains('?') { '&' } else { '?' };
    Some(format!("{url}{sep}options=-c%20search_path%3D{schema}"))
}

#[test]
fn postgres_sync_flow() {
    let Some(url) = database("flow") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-flow", |c| c.database = url.clone());
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    let wraps = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &k1);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": wraps }))).status, 201);
    let put = |d: &Device, version: u64, vv: serde_json::Value, epoch: u32, key: &crypto::WorkspaceKey, pt: &[u8]| {
        s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&d.session), Some(json!({ "version": version, "vv": vv, "epoch": epoch, "content": seal(key, WS, "a", version, epoch, pt) })))
    };
    assert_eq!(put(&ada, 1, json!({ "a": 1 }), 1, &k1, b"one").status, 200);
    assert_eq!(put(&bob, 1, json!({ "b": 1 }), 1, &k1, b"other").status, 409);
    assert_eq!(put(&bob, 2, json!({ "a": 1, "b": 1 }), 1, &k1, b"two").status, 200);
    let feed = s.call("GET", &format!("/v1/workspaces/{WS}/changes?since=0"), Some(&ada.session), None);
    assert_eq!(feed.json["changes"][0]["version"], 2);
    assert!(feed.json["changes"][0]["size"].as_i64().unwrap() > 40);

    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(put(&ada, 3, json!({ "a": 2, "b": 1 }), 1, &k1, b"stale").json["error"], "stale_epoch");
    assert_eq!(put(&ada, 3, json!({ "a": 2, "b": 1 }), 2, &k2, b"three").status, 200);
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&ada.session), None);
    assert_eq!(open(&ada.workspace_key(&s, WS, 2), WS, "a", 3, 2, got.json["content"].as_str().unwrap()), b"three");
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 404);
    assert_eq!(s.call("POST", "/v1/auth/logout", Some(&bob.session), None).status, 204);
}

#[test]
fn postgres_teams_flow() {
    let Some(url) = database("teams") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-teams", |c| c.database = url.clone());
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let inv = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "viewer" })));
    assert_eq!(inv.status, 201, "{}", inv.json);
    let bob = Device::new(&s, "bob@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": inv.json["token"] }))).status, 200);
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), None);
    assert_eq!(list.json["invites"][0]["accepted"]["email"], "bob@example.com");
    let w = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "viewer", "wraps": w }))).status, 201);
    // A format 2 wrap replaces Bob's format 1 wrap (upsert with length() on bytea), not the reverse.
    let legacy = |s: &Server| s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).json["members"].as_array().unwrap().iter().find(|m| m["email"] == "bob@example.com").unwrap()["keys"][0]["legacyEpochs"].clone();
    assert_eq!(legacy(&s), json!([1]));
    let v2 = crypto::seal_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &bob.key_id, public_key: &bob.keys.public_key() }, WS, 1, 1, &key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": [{ "keyId": bob.key_id, "wrapped": b64(&v2) }] })));
    assert_eq!(r.json["stored"], 1, "{}", r.json);
    assert_eq!(legacy(&s), json!([]));
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": w }))).json["stored"], 0);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "role": "editor" }))).status, 200);
    let kp = crypto::DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&kp.public_key(), WS, 1, &key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    let token = r.json["token"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&token), None).json["role"], "viewer");
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&token), Some(json!({ "id": WS, "wraps": [] }))).status, 403);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/me", Some(&token), None).status, 401);
}

#[test]
fn postgres_audit_and_sso() {
    use common::idp::{self, MockIdp};
    use hitop_sync_server::audit;
    let Some(url) = database("audit") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let idp = MockIdp::start();
    let s = Server::with("pg-audit", |c| {
        c.database = url.clone();
        c.oidc = Some(idp.config());
        c.audit_retention_days = 30;
    });
    let ada = Device::new(&s, "ada@example.com");
    create_workspace(&s, &ada, WS);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" }))).status, 201);
    // SSO links the existing account (oidc_identities on Postgres).
    let flow = sso_start(&s, Some("http://127.0.0.1:4000/"));
    idp.next(idp.sign(&idp.claims("sub-ada", "ada@example.com", &flow.nonce)), &flow.pkce);
    let code = sso_code(&sso_callback(&s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state))).unwrap();
    let session = sso_finish(&s, &code, &flow.verifier).json["session"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["id"], ada.user_id.as_str());
    let log = s.call("GET", &format!("/v1/workspaces/{WS}/audit?limit=1"), Some(&ada.session), None);
    assert_eq!((log.json["events"][0]["event"].as_str(), log.json["next"].as_i64()), (Some("invite_created"), Some(2)));
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/audit/verify"), Some(&ada.session), None).json["ok"], true);
    s.app.state().db.tx(|tx| tx.execute("UPDATE audit_log SET actor_email = 'x' WHERE workspace_id = $1 AND seq = 1", &[WS.into()])).unwrap();
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/audit/verify"), Some(&ada.session), None).json["ok"], false);
    s.advance(31 * 86_400);
    assert!(audit::purge(s.app.state()).unwrap() >= 2);
    assert!(audit::verify_all(s.app.state()).unwrap().iter().all(|(_, v)| v.problem.is_none()), "purged chains verify from their anchors");
}

/// Migration 3 → 4 of a T25-shaped database on Postgres (common/t25.rs, as tests/migration.rs).
#[test]
fn postgres_t25_database_migrates_to_orgs() {
    let Some(url) = database("t25") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    common::t25::build(&url);
    let before = common::t25::counts(&hitop_sync_server::Db::open_at(&url, 3).unwrap());
    let s = Server::with("pg-t25", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    common::t25::check(&s, &before);
}

/// Orgs, members, usage, limits, a refused create and access tokens on Postgres.
#[test]
fn postgres_orgs_limits_tokens() {
    let Some(url) = database("orgs") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-orgs", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let org = s.call("POST", "/v1/orgs", Some(&ada.session), Some(json!({ "name": "Acme" })));
    assert_eq!(org.status, 201, "{}", org.json);
    let id = org.json["id"].as_str().unwrap().to_string();
    assert_eq!(s.call("POST", &format!("/v1/orgs/{id}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "admin" }))).status, 201);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{id}/members"), Some(&bob.session), Some(json!({ "email": "new@example.com", "role": "member" }))).json["status"], "invited");
    for i in 0..3 {
        let ws = format!("{i:032x}");
        let key = crypto::WorkspaceKey::generate();
        let wraps = wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], &ws, 1, &key);
        let r = s.call("POST", &format!("/v1/orgs/{id}/workspaces"), Some(&ada.session), Some(json!({ "id": ws, "wraps": wraps })));
        assert_eq!(r.status, 201, "{}", r.json);
        let put = s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/a"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": 1 }, "epoch": 1, "content": seal(&key, &ws, "a", 1, 1, b"one") })));
        assert_eq!(put.status, 200, "{}", put.json);
    }
    let key = crypto::WorkspaceKey::generate();
    let ws = format!("{:032x}", 9);
    let r = s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": ws, "org": id, "wraps": wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], &ws, 1, &key) })));
    assert_eq!((r.status, r.json["limit"].as_str()), (402, Some("workspaces")), "{}", r.json);
    let u = s.call("GET", &format!("/v1/orgs/{id}/usage"), Some(&bob.session), None);
    assert_eq!(u.status, 200, "{}", u.json);
    assert_eq!((u.json["usage"]["workspaces"].as_i64(), u.json["usage"]["seats"].as_i64(), u.json["usage"]["writesToday"].as_i64()), (Some(3), Some(3), Some(3)));
    assert!(u.json["usage"]["storageBytes"].as_i64().unwrap() > 0);
    let over = s.app.state().db.tx(|tx| hitop_sync_server::limits::over_limits(s.app.state(), tx)).unwrap();
    assert!(over.is_empty(), "{over:?}");
    hitop_sync_server::limits::set_plan(s.app.state(), &id, "team", 5).unwrap();
    assert_eq!(s.call("GET", &format!("/v1/orgs/{id}/limits"), Some(&ada.session), None).json["limits"]["seats"], 5);
    let t = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read"], "expiresInDays": 30 })));
    assert_eq!(t.status, 201, "{}", t.json);
    let token = t.json["token"].as_str().unwrap();
    assert_eq!(s.call("GET", "/v1/workspaces", Some(token), None).json["workspaces"].as_array().unwrap().len(), 3);
    assert_eq!(s.call("DELETE", &format!("/v1/me/tokens/{}", t.json["id"].as_str().unwrap()), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/workspaces", Some(token), None).status, 401);
}

fn bin(args: &[&str], database: Option<&str>) -> std::process::Output {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_hitop-sync-server"));
    c.args(args).env_remove("HITOP_SYNC_DATABASE");
    if let Some(db) = database {
        c.env("HITOP_SYNC_DATABASE", db);
    }
    c.output().unwrap()
}

fn output(o: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// HITOP Cloud C2: the switch from SQLite to Postgres (`migrate-db`), checked as the cutover script
/// checks it, and the way back (Postgres → a new SQLite file) for a rollback after writes.
#[test]
fn postgres_migrate_db_from_sqlite_and_back() {
    let Some(url) = database("migrate") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::new("pg-migrate-src");
    let chains = common::fill::fill(&s);
    let src = format!("sqlite:{}", s.dir.join("sync.db").display());
    let before = hitop_sync_server::dbtools::stats(&s.app.state().db).unwrap();

    let o = bin(&["migrate-db", "--from", &src, "--to", &url], None);
    assert_eq!(o.status.code(), Some(0), "{}", output(&o));
    assert!(output(&o).contains(&format!("{chains} chains, 0 broken")), "{}", output(&o));
    let pg = hitop_sync_server::Db::open(&url).unwrap();
    assert_eq!(pg.kind(), "postgres");
    assert_eq!(hitop_sync_server::dbtools::stats(&pg).unwrap().tables, before.tables, "every row, byte for byte");
    drop(pg);
    let o = bin(&["audit-verify"], Some(&url));
    assert_eq!(o.status.code(), Some(0), "{}", output(&o));

    // A second run is refused: the target has rows now.
    let o = bin(&["migrate-db", "--from", &src, "--to", &url], None);
    assert_eq!(o.status.code(), Some(2), "{}", output(&o));
    assert!(output(&o).contains("not empty"), "{}", output(&o));

    // The server runs on the copy: people sign in and find their workspace, writes go on.
    let after = Server::with("pg-migrate-run", |c| c.database = url.clone());
    let session = after.sign_in("ada@example.com");
    let mine = after.call("GET", "/v1/workspaces", Some(&session), None);
    assert_eq!(mine.json["workspaces"].as_array().unwrap().len(), 1, "{}", mine.json);
    Device::new(&after, "dee@example.com");
    let grown = hitop_sync_server::dbtools::stats(&after.app.state().db).unwrap();
    drop(after);

    // Rollback after writes on Postgres: copied back into a new SQLite file, nothing lost.
    let back = format!("sqlite:{}", s.dir.join("back.db").display());
    let o = bin(&["migrate-db", "--from", &url, "--to", &back], None);
    assert_eq!(o.status.code(), Some(0), "{}", output(&o));
    assert_eq!(hitop_sync_server::dbtools::stats(&hitop_sync_server::Db::open(&back).unwrap()).unwrap().tables, grown.tables);
}

/// `hitop-sync-server dump` on Postgres: pg_dump runs in the snapshot the manifest's stats come
/// from, and a pg_restore of it into a new database has exactly those rows (what restore-drill.sh
/// checks). Needs HITOP_SYNC_TEST_PG_DUMP and HITOP_SYNC_TEST_PG_RESTORE (the commands; CI runs
/// them from the postgres:17-alpine image, as the server's major version).
#[test]
fn postgres_dump_matches_its_manifest_after_a_restore() {
    let (Ok(url), Ok(pg_dump), Ok(pg_restore)) = (std::env::var("HITOP_SYNC_TEST_POSTGRES"), std::env::var("HITOP_SYNC_TEST_PG_DUMP"), std::env::var("HITOP_SYNC_TEST_PG_RESTORE")) else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES, HITOP_SYNC_TEST_PG_DUMP or HITOP_SYNC_TEST_PG_RESTORE not set: skipped");
        return;
    };
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).unwrap();
    let (src_db, dst_db) = (format!("dump_src_{}", std::process::id()), format!("dump_dst_{}", std::process::id()));
    for d in [&src_db, &dst_db] {
        admin.batch_execute(&format!("DROP DATABASE IF EXISTS {d}")).unwrap();
        admin.batch_execute(&format!("CREATE DATABASE {d}")).unwrap();
    }
    let with_db = |d: &str| {
        let mut u = url::Url::parse(&url).unwrap();
        u.set_path(&format!("/{d}"));
        u.to_string()
    };
    let src = with_db(&src_db);
    let s = Server::with("pg-dump", |c| c.database = src.clone());
    let chains = common::fill::fill(&s);
    let dir = s.dir.join("dump");
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_hitop-sync-server")).args(["dump", dir.to_str().unwrap()]).env("HITOP_SYNC_DATABASE", &src).env("HITOP_SYNC_PG_DUMP", &pg_dump).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}", output(&o));
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!((manifest["format"].as_str(), manifest["file"].as_str()), (Some("pg_dump-custom"), Some("sync.pgdump")));
    let before = hitop_sync_server::dbtools::stats(&s.app.state().db).unwrap();
    assert_eq!(manifest["stats"]["tables"], before.to_json()["tables"]);

    // Restore into an empty database with the given pg_restore (reading the dump on stdin).
    let dst = with_db(&dst_db);
    let words: Vec<&str> = pg_restore.split_whitespace().collect();
    let mut parsed = url::Url::parse(&dst).unwrap();
    let password = parsed.password().unwrap_or("").to_string();
    parsed.set_password(None).unwrap();
    let r = std::process::Command::new(words[0])
        .args(&words[1..])
        .args(["--no-owner", "--no-privileges", &format!("--dbname={parsed}")])
        .env("PGPASSWORD", password)
        .stdin(std::fs::File::open(dir.join("sync.pgdump")).unwrap())
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", output(&r));
    let restored = hitop_sync_server::Db::open(&dst).unwrap();
    assert_eq!(hitop_sync_server::dbtools::stats(&restored).unwrap().to_json()["tables"], manifest["stats"]["tables"]);
    drop(restored);
    let o = bin(&["audit-verify"], Some(&dst));
    assert!(output(&o).contains(&format!("{chains} chains, 0 broken")), "{}", output(&o));
    drop(s);
    for d in [&src_db, &dst_db] {
        admin.batch_execute(&format!("DROP DATABASE IF EXISTS {d} WITH (FORCE)")).unwrap();
    }
}

/// The unix socket URL form the VM uses (`?host=/run/postgresql`), when the test is given one.
#[test]
fn postgres_over_a_unix_socket() {
    let Ok(socket_url) = std::env::var("HITOP_SYNC_TEST_POSTGRES_SOCKET") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES_SOCKET not set: skipped");
        return;
    };
    let db = hitop_sync_server::Db::open(&socket_url).unwrap();
    assert_eq!(db.kind(), "postgres");
    assert_eq!(db.tx(|tx| tx.query_one("SELECT 1::bigint", &[])).unwrap().unwrap().int(0), 1);
}

#[test]
fn postgres_passwords() {
    let Some(url) = database("passwords") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-passwords", |c| c.database = url.clone());
    let (session, bundle_key) = common::pw::sign_up(&s, "ada@example.com", "a long password");
    let id = s.call("GET", "/v1/me", Some(&session), None).json["id"].as_str().unwrap().to_string();
    let bundle = crypto::seal_key_bundle(&bundle_key, &id, b"bundle").unwrap();
    assert_eq!(s.call("PUT", "/v1/me/recovery?method=password", Some(&session), Some(json!({ "blob": b64(&bundle) }))).status, 200);
    assert_eq!(common::pw::login(&s, "ada@example.com", "a long password").status, 200);
    assert_eq!(common::pw::login(&s, "ada@example.com", "the wrong one").status, 401);
    assert_eq!(common::pw::login(&s, "nobody@example.com", "the wrong one").status, 401);
    let params = |e: &str| s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": e }))).json;
    assert_eq!(params("nobody@example.com"), params("nobody@example.com"));
    assert_eq!(s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "ada@example.com" }))).status, 202);
    let (kdf, keys) = common::pw::new_password("a new password");
    let r = s.call("POST", "/v1/auth/password/reset", None, Some(json!({ "token": s.mailed_token("ada@example.com"), "kdf": common::pw::kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!((r.status, r.json["bundleLost"].clone()), (200, json!(true)), "{}", r.json);
    assert_eq!(common::pw::login(&s, "ada@example.com", "a new password").status, 200);
}

#[test]
fn postgres_billing() {
    use common::lemonsqueezy::{self as ls, delivered, MockLs};
    let Some(url) = database("billing") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let mock = MockLs::start();
    let config = mock.config();
    let s = Server::with("pg-billing", |c| {
        c.database = url.clone();
        c.plan_limits = true;
        c.billing = Some(config);
    });
    let ada = Device::new(&s, "ada@acme.io");
    let r = s.call("POST", "/v1/orgs", Some(&ada.session), Some(json!({ "name": "Acme" })));
    let org = r.json["id"].as_str().unwrap().to_string();
    // The trial and its end.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/trial"), Some(&ada.session), Some(json!({}))).status, 201);
    s.advance(15 * 86_400);
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&ada.session), None).json["planInForce"], "free");
    // A purchase, a replay, a stale update.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team", "seats": 3 }))).status, 200);
    let custom = mock.last("POST").body["data"]["attributes"]["checkout_data"]["custom"].clone();
    let created = ls::subscription("subscription_created", "1", ls::TEAM_ANNUAL, "active", 3, "2026-11-20T10:00:00.000000Z", custom.clone());
    delivered(&s, &created, "applied");
    delivered(&s, &created, "duplicate");
    delivered(&s, &ls::subscription("subscription_updated", "1", ls::TEAM_ANNUAL, "expired", 3, "2026-11-19T10:00:00.000000Z", custom), "stale");
    let l = s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&ada.session), None);
    assert_eq!((l.json["planInForce"].as_str(), l.json["seats"].as_i64()), (Some("team"), Some(3)));
    delivered(&s, &ls::invoice("subscription_payment_refunded", "1", true, "2026-11-21T10:00:00.000000Z"), "applied");
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&ada.session), None).json["planInForce"], "free");
}

#[test]
fn postgres_runs_alerts_and_shares() {
    let Some(url) = database("runs") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-runs", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    let ada = Device::new(&s, "ada@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    let org = format!("personal-{}", ada.user_id);
    hitop_sync_server::limits::set_plan(s.app.state(), &org, "pro", 0).unwrap();
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "email": ["ada@example.com"], "missedAfterMinutes": 60 }))).status, 200);
    for (id, status) in [("r1", "passed"), ("r2", "failed")] {
        let detail = crypto::seal_run(&k1, WS, id, 1, b"{\"reportVersion\":1}").unwrap();
        let r = s.call("POST", &format!("/v1/workspaces/{WS}/runs"), Some(&ada.session), Some(json!({ "id": id, "startedAt": 1, "collection": "c", "environment": "e", "status": status, "requests": 2, "failed": u8::from(status == "failed"), "detail": b64(&detail) })));
        assert_eq!((r.status, r.json["stored"].as_str()), (201, Some("status+report")), "{}", r.json);
    }
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/runs"), Some(&ada.session), None);
    assert_eq!(list.json["runs"].as_array().unwrap().len(), 2);
    let one = s.call("GET", &format!("/v1/workspaces/{WS}/runs/r2"), Some(&ada.session), None);
    assert_eq!(crypto::open_run(&ada.workspace_key(&s, WS, 1), WS, "r2", &unb64(one.json["detail"].as_str().unwrap())).unwrap(), b"{\"reportVersion\":1}");
    assert_eq!(hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap(), 1);
    s.advance(2 * 3600);
    assert_eq!(hitop_sync_server::alerts::tick(s.app.state()).unwrap(), 1);
    s.advance(31 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 2);
    let ada = Device::new(&s, "ada@example.com");
    let share = "0123456789abcdef0123456789abcdef";
    let key = crypto::WorkspaceKey::generate();
    let pk = ada.keys.public_key();
    let wrap = crypto::seal_share_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &ada.key_id, public_key: &pk }, WS, share, 1, &key).unwrap();
    let sealed = crypto::seal_share(&key, WS, share, 1, b"{}").unwrap();
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/env-shares/{share}"), Some(&ada.session), Some(json!({ "version": 1, "sealed": b64(&sealed), "wraps": [{ "keyId": ada.key_id, "wrapped": b64(&wrap) }] })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&ada.session), None).json["shares"][0]["version"], 1);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 204, "the workspace goes with its runs, alerts and shares");
}
