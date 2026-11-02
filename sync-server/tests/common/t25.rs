//! A database as the test server has it before migration 4 (schema 3, T25), with an account that
//! is over the Free limits on day one: Ada owns 7 workspaces (150 MB), shares them with 5 people,
//! has 6 devices and 6 runner credentials. Written with plain SQL through `Db::open_at`, so it
//! works on SQLite and Postgres.

use hitop_sync_server::Db;

pub const ADA_SESSION: &str = "ada-session-token-from-before-migration-4";
pub const BOB_SESSION: &str = "bob-session-token-from-before-migration-4";
pub const PEOPLE: [&str; 6] = ["ada", "bob", "cy", "dan", "eve", "fay"];
pub const MB: i64 = 1 << 20;

pub fn ws_id(i: usize) -> String {
    format!("{i:032x}")
}

fn hash(token: &str) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Fills `url` (an empty database) with the schema-3 data above.
pub fn build(url: &str) {
    let db = Db::open_at(url, 3).unwrap();
    db.tx(|tx| {
        for (i, p) in PEOPLE.iter().enumerate() {
            tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &[format!("u-{p}").into(), format!("{p}@example.com").into(), (1000 + i as i64).into()])?;
            tx.execute("INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ($1, $2, 'laptop', $3, 1)", &[format!("k-{p}-0").into(), format!("u-{p}").into(), vec![7u8; 32].into()])?;
        }
        for d in 1..6 {
            tx.execute("INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ($1, 'u-ada', 'another', $2, 1)", &[format!("k-ada-{d}").into(), vec![d as u8; 32].into()])?;
        }
        let far = 4_000_000_000i64;
        tx.execute("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ($1, 'u-ada', 1, $2)", &[hash(ADA_SESSION).into(), far.into()])?;
        tx.execute("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ($1, 'u-bob', 1, $2)", &[hash(BOB_SESSION).into(), far.into()])?;
        for w in 0..7 {
            let ws = ws_id(w);
            let bytes = if w == 0 { 150 * MB - 6 * 1000 } else { 1000 };
            tx.execute("INSERT INTO workspaces (id, epoch, seq, bytes, created_at) VALUES ($1, 1, 1, $2, $3)", &[(&ws).into(), bytes.into(), (2000 + w as i64).into()])?;
            tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, 'u-ada', 'owner')", &[(&ws).into()])?;
            tx.execute("INSERT INTO wraps (workspace_id, epoch, key_id, wrapped) VALUES ($1, 1, 'k-ada-0', $2)", &[(&ws).into(), vec![1u8; 80].into()])?;
            let envelope: Vec<u8> = [vec![1u8, 0, 0, 0, 1], vec![9u8; 60]].concat();
            tx.execute(
                "INSERT INTO blobs (workspace_id, id, version, vv, epoch, deleted, content, seq, updated_at) VALUES ($1, 'b1', 1, '{\"a\":1}', 1, 0, $2, 1, 1)",
                &[(&ws).into(), envelope.into()],
            )?;
        }
        // Five collaborators in the first workspace (two editors, three viewers), one in another.
        for (p, role) in [("bob", "editor"), ("cy", "editor"), ("dan", "viewer"), ("eve", "viewer"), ("fay", "viewer")] {
            tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, $2, $3)", &[ws_id(0).into(), format!("u-{p}").into(), role.into()])?;
        }
        tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, 'u-bob', 'editor')", &[ws_id(1).into()])?;
        // Bob owns one workspace of his own.
        tx.execute("INSERT INTO workspaces (id, epoch, seq, bytes, created_at) VALUES ($1, 1, 0, 0, 3000)", &[ws_id(9).into()])?;
        tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, 'u-bob', 'owner')", &[ws_id(9).into()])?;
        // Six runner credentials over two of Ada's workspaces.
        for r in 0..6 {
            let id = format!("r-{r}");
            let ws = ws_id(r % 2);
            tx.execute("INSERT INTO users (id, email, created_at, runner_of) VALUES ($1, $2, 1, $3)", &[(&id).into(), format!("{id}@runner.invalid").into(), (&ws).into()])?;
            tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, $2, 'viewer')", &[(&ws).into(), (&id).into()])?;
        }
        tx.execute(
            "INSERT INTO invites (id, workspace_id, token_hash, email, role, created_by, created_at, expires_at) VALUES ('i1', $1, 'h1', 'zed@example.com', 'viewer', 'u-ada', 1, $2)",
            &[ws_id(0).into(), far.into()],
        )?;
        tx.execute("INSERT INTO oidc_identities (issuer, subject, user_id, email, created_at, last_used_at) VALUES ('https://id.example', 's-ada', 'u-ada', 'ada@example.com', 1, 1)", &[])?;
        tx.execute(
            "INSERT INTO audit_log (workspace_id, seq, at, event, actor_id, actor_email, target, detail, ip, user_agent, prev_hash, hash) VALUES ('', 1, 1, 'signin', 'u-ada', 'ada@example.com', 'u-ada', '{}', '', '', $1, 'h')",
            &[hitop_sync_server::audit::GENESIS.into()],
        )?;
        Ok::<_, hitop_sync_server::db::DbError>(())
    })
    .unwrap();
}

/// Row counts of the tables migration 4 must leave alone.
pub fn counts(db: &Db) -> Vec<(String, i64)> {
    ["users", "sessions", "device_keys", "workspaces", "members", "wraps", "blobs", "invites", "oidc_identities", "audit_log"]
        .iter()
        .map(|t| (t.to_string(), db.tx(|tx| tx.query_one(&format!("SELECT COUNT(*) FROM {t}"), &[])).unwrap().unwrap().int(0)))
        .collect()
}

fn count(db: &Db, sql: &str) -> i64 {
    db.tx(|tx| tx.query_one(sql, &[])).unwrap().unwrap().int(0)
}

fn wraps(n: usize, owner: &str) -> serde_json::Value {
    let keys: Vec<String> = if owner == "ada" { (0..n).map(|d| format!("k-ada-{d}")).collect() } else { vec![format!("k-{owner}-0")] };
    serde_json::Value::Array(keys.iter().map(|k| serde_json::json!({ "keyId": k, "wrapped": super::b64(&[3u8; 80]) })).collect())
}

/// What migration 4 must have done to `build`'s database (`s` runs on it with plan limits on):
/// no existing row lost, one personal org per person (none for runners), workspaces in their
/// owner's org, nobody over a limit, every read still answered, and creates refused only beyond
/// the grandfathered usage.
pub fn check(s: &super::Server, before: &[(String, i64)]) {
    use serde_json::json;
    let db = &s.app.state().db;
    assert_eq!(counts(db), before, "migration 4 keeps every existing row");
    let people = count(db, "SELECT COUNT(*) FROM users WHERE runner_of = ''");
    assert_eq!(people, 6);
    assert_eq!(count(db, "SELECT COUNT(*) FROM orgs WHERE personal_of <> ''"), people, "accounts before = personal orgs after");
    assert_eq!(count(db, "SELECT COUNT(*) FROM org_members"), people);
    assert_eq!(count(db, "SELECT COUNT(*) FROM entitlements WHERE plan = 'free'"), people);
    assert_eq!(count(db, "SELECT COUNT(*) FROM workspaces WHERE org_id = ''"), 0);
    assert_eq!(count(db, "SELECT COUNT(*) FROM workspaces WHERE org_id = 'personal-u-ada'"), 7);
    let over = db.tx(|tx| hitop_sync_server::limits::over_limits(s.app.state(), tx)).unwrap();
    assert!(over.is_empty(), "nobody is over a limit after the migration: {over:?}");

    let orgs = s.call("GET", "/v1/orgs", Some(ADA_SESSION), None);
    assert_eq!(orgs.status, 200, "{}", orgs.json);
    assert_eq!((orgs.json["orgs"][0]["id"].as_str(), orgs.json["orgs"][0]["personal"].as_bool(), orgs.json["orgs"][0]["plan"].as_str()), (Some("personal-u-ada"), Some(true), Some("free")));
    let u = s.call("GET", "/v1/orgs/personal-u-ada/usage", Some(ADA_SESSION), None);
    assert_eq!(u.status, 200, "{}", u.json);
    assert_eq!(u.json["grandfathered"], true);
    for (metric, used) in [("workspaces", 7), ("seats", 6), ("devices", 6), ("runners", 6), ("storageBytes", 150 * MB)] {
        assert_eq!((metric, &u.json["limits"][metric]), (metric, &json!(used)), "limit = usage");
        assert_eq!((metric, &u.json["usage"][metric]), (metric, &json!(used)));
    }
    assert_eq!(u.json["limits"]["invitesPerDay"], 20, "not grandfathered: under Free");

    // Reads and pulls: all answered.
    let list = s.call("GET", "/v1/workspaces", Some(ADA_SESSION), None);
    assert_eq!(list.json["workspaces"].as_array().unwrap().len(), 7);
    assert!(list.json["workspaces"].as_array().unwrap().iter().all(|w| w["org"]["personal"] == true));
    let ws0 = ws_id(0);
    for path in [format!("/v1/workspaces/{ws0}"), format!("/v1/workspaces/{ws0}/changes?since=0"), format!("/v1/workspaces/{ws0}/blobs/b1"), format!("/v1/workspaces/{ws0}/wraps?keyId=k-ada-0"), "/v1/me".into()] {
        assert_eq!(s.call("GET", &path, Some(ADA_SESSION), None).status, 200, "{path}");
    }
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{ws0}/blobs/b1"), Some(BOB_SESSION), None).status, 200, "a collaborator reads too");

    // Creates beyond the grandfathered usage are refused, with the limit named.
    let new_ws = s.call("POST", "/v1/workspaces", Some(ADA_SESSION), Some(json!({ "id": ws_id(20), "wraps": wraps(6, "ada") })));
    assert_eq!(new_ws.status, 402, "{}", new_ws.json);
    assert_eq!((new_ws.json["error"].as_str(), new_ws.json["limit"].as_str(), new_ws.json["plan"].as_str()), (Some("limit"), Some("workspaces"), Some("free")));
    assert_eq!((new_ws.json["used"].as_i64(), new_ws.json["max"].as_i64()), (Some(7), Some(7)));
    assert!(new_ws.json["upgradeUrl"].as_str().unwrap().contains("personal-u-ada"));
    assert!(new_ws.json["message"].as_str().unwrap().contains("free plan"), "{}", new_ws.json["message"]);
    let key = s.call("POST", "/v1/me/keys", Some(ADA_SESSION), Some(json!({ "name": "seventh", "publicKey": super::b64(&[8u8; 32]) })));
    assert_eq!((key.status, key.json["limit"].as_str()), (402, Some("devices")), "{}", key.json);
    // A write that doesn't grow storage goes through; one that grows it doesn't.
    let envelope = |extra: usize| super::b64(&[vec![1u8, 0, 0, 0, 1], vec![9u8; 60 + extra]].concat());
    let same = s.call("PUT", &format!("/v1/workspaces/{ws0}/blobs/b1"), Some(ADA_SESSION), Some(json!({ "version": 2, "vv": { "a": 2 }, "epoch": 1, "content": envelope(0) })));
    assert_eq!(same.status, 200, "{}", same.json);
    let grow = s.call("PUT", &format!("/v1/workspaces/{ws0}/blobs/b1"), Some(ADA_SESSION), Some(json!({ "version": 3, "vv": { "a": 3 }, "epoch": 1, "content": envelope(5000) })));
    assert_eq!((grow.status, grow.json["limit"].as_str()), (402, Some("storageBytes")), "{}", grow.json);
    // Deleting is never refused, and frees room.
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{}", ws_id(6)), Some(ADA_SESSION), None).status, 204);
    assert_eq!(s.call("POST", "/v1/workspaces", Some(ADA_SESSION), Some(json!({ "id": ws_id(20), "wraps": wraps(6, "ada") }))).status, 201);
    // Bob, under Free, creates as before.
    assert_eq!(s.call("POST", "/v1/workspaces", Some(BOB_SESSION), Some(json!({ "id": ws_id(21), "wraps": wraps(1, "bob") }))).status, 201);
}
