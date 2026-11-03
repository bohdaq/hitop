//! `hitop-sync-server support-bundle` on a self-hosted instance (SQLite, no plans) filled through
//! the API (tests/common/fill.rs: accounts, workspaces, orgs, SSO, SCIM, runs, ...): the bundle has
//! versions, the schema, redacted settings, row counts and scrubbed error lines, and no customer
//! datum or secret: no address, id, name, token or hash from the database, no setting's secret.

mod common;

use std::process::Command;

use common::{fill, Server};

const SECRETS: &[(&str, &str)] = &[
    ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_verysecret1"),
    ("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_apikeyvalue2"),
    ("HITOP_SYNC_STRIPE_PRICE_TEAM_ANNUAL", "price_team777"),
    ("HITOP_SYNC_HOST_KEY", "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12"),
    ("HITOP_SYNC_PASSWORD_PEPPER", "pepper-value-3-long-enough-for-it"),
    ("HITOP_SYNC_GITHUB_CLIENT_ID", "gh-client-id-4"),
    ("HITOP_SYNC_GITHUB_CLIENT_SECRET", "gh-client-secret-5"),
    ("SMTP_URL", "smtp://mailer:smtp-password-6@mail.example.net:587"),
    ("HITOP_SYNC_SOME_FUTURE_SETTING", "future-value-7"),
];

#[test]
fn a_support_bundle_has_what_support_needs_and_nothing_of_the_customers() {
    let s = Server::new("support-bundle");
    let chains = fill::fill(&s);
    let db = format!("sqlite:{}", s.dir.join("sync.db").display());
    let log = s.dir.join("server.log");
    std::fs::write(
        &log,
        format!(
            "hitop-sync-server: SSO on (callback https://sync.example.com/v1/auth/sso/callback)\n\
             mail to ada@example.com: Sign in to hitop sync\n\
             failed to send for ada@example.com\n\
             hitop-sync-server: alert {}-1 (webhook): failed: connection refused by 10.0.0.7\n\
             hitop-sync-server: audit purge: database is locked\n\
             hitop-sync-server: Lemon Squeezy POST /v1/checkouts: 500 token=sk_test_apikeyvalue2abcdef1234\n",
            fill::WS
        ),
    )
    .unwrap();
    let out = s.dir.join("bundle.json");
    let mut c = Command::new(env!("CARGO_BIN_EXE_hitop-sync-server"));
    c.args(["support-bundle", "--out", out.to_str().unwrap(), "--log", log.to_str().unwrap()]).env("HITOP_SYNC_DATABASE", &db).env("HITOP_SYNC_PUBLIC_URL", "https://sync.example.com").env("HITOP_SYNC_LIMITS", "off");
    for (k, v) in SECRETS {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let text = std::fs::read_to_string(&out).unwrap();
    let b: serde_json::Value = serde_json::from_str(&text).unwrap();

    // What support needs.
    assert_eq!(b["versions"]["server"], env!("CARGO_PKG_VERSION"));
    assert_eq!(b["versions"]["schemaInDatabase"], hitop_sync_server::Db::schema_version() as i64);
    assert_eq!(b["versions"]["schemaOfThisBinary"], hitop_sync_server::Db::schema_version());
    assert_eq!(b["versions"]["database"], "sqlite");
    assert!(b["versions"]["databaseVersion"].as_str().unwrap().starts_with('3'));
    assert_eq!(b["config"]["settings"]["HITOP_SYNC_LIMITS"], "off");
    assert_eq!(b["config"]["settings"]["HITOP_SYNC_PUBLIC_URL"], "https://sync.example.com/");
    assert_eq!(b["config"]["settings"]["HITOP_SYNC_STRIPE_PRICE_TEAM_ANNUAL"], "price_team777");
    assert_eq!(b["config"]["settings"]["SMTP_URL"], "smtp://mail.example.net:587");
    for (k, _) in SECRETS.iter().filter(|(k, _)| !k.contains("PRICE") && *k != "SMTP_URL") {
        assert_eq!(b["config"]["settings"][k], "(set)", "{k}");
    }
    assert_eq!(b["config"]["in effect"]["billing"], true);
    assert_eq!(b["config"]["in effect"]["hostKey"], true);
    let tables = b["database"]["tables"].as_array().unwrap();
    assert_eq!(tables.len(), hitop_sync_server::Db::tables().len());
    assert!(tables.iter().any(|t| t["table"] == "scim_users" && t["rows"] == 2));
    assert!(tables.iter().all(|t| t.get("hash").is_none()), "row counts only");
    assert_eq!(b["health"]["auditChains"], chains);
    assert_eq!(b["health"]["auditChainsBroken"], 0);
    let errors: Vec<&str> = b["recentErrors"]["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(errors.iter().any(|l| l.contains("database is locked")));
    assert!(errors.iter().any(|l| l.contains("<email>")));
    assert!(errors.iter().any(|l| l.contains("<id>") && l.contains("<ip>")), "{errors:?}");

    // Nothing of the customers': every text value in the identifying columns, every secret.
    let mut forbidden: Vec<String> = SECRETS.iter().filter(|(k, _)| !k.contains("PRICE")).map(|(_, v)| v.to_string()).collect();
    forbidden.extend(["smtp-password-6".to_string(), "mailer".to_string()]);
    let columns = [
        ("users", "id"),
        ("users", "email"),
        ("sessions", "token_hash"),
        ("device_keys", "id"),
        ("workspaces", "id"),
        ("orgs", "id"),
        ("orgs", "name"),
        ("org_invites", "email"),
        ("invites", "email"),
        ("invites", "token_hash"),
        ("api_tokens", "token_hash"),
        ("blobs", "id"),
        ("runs", "id"),
        ("runs", "collection"),
        ("scim_tokens", "token_hash"),
        ("scim_users", "user_name"),
        ("scim_users", "id"),
        ("scim_groups", "display_name"),
        ("org_domains", "domain"),
        ("org_sso", "issuer"),
        ("audit_chain", "head_hash"),
    ];
    let state = s.app.state();
    for (table, column) in columns {
        let rows = state.db.tx(|tx| tx.query(&format!("SELECT {column} FROM {table}"), &[])).unwrap_or_else(|e| panic!("{table}.{column}: {e}"));
        assert!(!rows.is_empty(), "{table} has rows");
        for r in rows {
            let v = r.text(0);
            if v.len() >= 4 && v != "Personal" {
                forbidden.push(v);
            }
        }
    }
    for f in &forbidden {
        assert!(!text.contains(f.as_str()), "the bundle contains {f}");
    }

    // An existing file is never overwritten; without a journal the bundle says so.
    let o = Command::new(env!("CARGO_BIN_EXE_hitop-sync-server")).args(["support-bundle", "--out", out.to_str().unwrap(), "--no-journal"]).env("HITOP_SYNC_DATABASE", &db).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    let second = s.dir.join("b2.json");
    let o = Command::new(env!("CARGO_BIN_EXE_hitop-sync-server")).args(["support-bundle", "--out", second.to_str().unwrap(), "--no-journal"]).env("HITOP_SYNC_DATABASE", &db).output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    let b2: serde_json::Value = serde_json::from_slice(&std::fs::read(&second).unwrap()).unwrap();
    assert_eq!(b2["recentErrors"]["source"], "none");
}
