mod common;

use common::{Server, ORIGIN};
use serde_json::json;
use url::Url;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_rs::prelude::{CreationChallengeResponse, RequestChallengeResponse};

#[test]
fn magic_link_signs_in_once() {
    let s = Server::new("magic");
    let r = s.call("POST", "/v1/auth/email", None, Some(json!({ "email": " Ada@Example.com " })));
    assert_eq!(r.status, 202, "{}", r.json);
    let mail = s.outbox.lock().unwrap().last().cloned().unwrap();
    assert_eq!(mail.to, "ada@example.com");
    assert!(mail.body.contains("https://sync.test.example/signin#"), "{}", mail.body);
    let token = s.mailed_token("ada@example.com");

    let r = s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": token })));
    assert_eq!(r.status, 200, "{}", r.json);
    let session = r.json["session"].as_str().unwrap().to_string();
    assert_eq!(r.json["user"]["email"], "ada@example.com");
    let me = s.call("GET", "/v1/me", Some(&session), None);
    assert_eq!(me.status, 200);
    assert_eq!(me.json["email"], "ada@example.com");

    // Single use.
    assert_eq!(s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": token }))).status, 401);
    // A second link signs in to the same account.
    let again = s.sign_in("ada@example.com");
    assert_eq!(s.call("GET", "/v1/me", Some(&again), None).json["id"], me.json["id"]);
}

#[test]
fn links_and_sessions_expire() {
    let s = Server::new("expire");
    s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "bo@example.com" })));
    let token = s.mailed_token("bo@example.com");
    s.advance(16 * 60);
    assert_eq!(s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": token }))).status, 401);

    let session = s.sign_in("bo@example.com");
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).status, 200);
    s.advance(31 * 24 * 3600);
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).status, 401);
}

#[test]
fn rejects_bad_tokens_and_logs_out() {
    let s = Server::new("tokens");
    assert_eq!(s.call("GET", "/v1/me", None, None).status, 401);
    assert_eq!(s.call("GET", "/v1/me", Some("not-a-session"), None).status, 401);
    assert_eq!(s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": "guess" }))).status, 401);
    let session = s.sign_in("cy@example.com");
    assert_eq!(s.call("POST", "/v1/auth/logout", Some(&session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).status, 401);
}

#[test]
fn validates_addresses_and_rate_limits() {
    let s = Server::new("ratelimit");
    for bad in ["", "no-at-sign", "a@b\r\nBcc: x@evil.example", "two words@example.com"] {
        assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": bad }))).status, 400, "{bad:?}");
    }
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "a@example.com", "extra": 1 }))).status, 400);
    for _ in 0..5 {
        assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@example.com" }))).status, 202);
    }
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@example.com" }))).status, 429);
    // Other addresses aren't affected; an hour later the address works again.
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "eve@example.com" }))).status, 202);
    s.advance(3601);
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@example.com" }))).status, 202);
}

#[test]
fn email_sign_in_off_without_smtp() {
    let dir = std::env::temp_dir().join(format!("hitop-sync-test-nomail-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut config = hitop_sync_server::Config::local(ORIGIN);
    config.database = "sqlite::memory:".into();
    let app = hitop_sync_server::SyncApp::new(hitop_sync_server::State::new(hitop_sync_server::Db::open(&config.database).unwrap(), hitop_sync_server::Mailer::Off, config).unwrap());
    let s = Server { app, outbox: Default::default(), dir };
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "a@example.com" }))).status, 503);
}

#[test]
fn stores_only_token_hashes() {
    let s = Server::new("hashes");
    s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "fay@example.com" })));
    let link = s.mailed_token("fay@example.com");
    let session = s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": link }))).json["session"].as_str().unwrap().to_string();
    let disk = s.disk_bytes();
    for secret in [&link, &session] {
        assert!(!disk.windows(secret.len()).any(|w| w == secret.as_bytes()), "a token is stored in clear");
    }
}

#[test]
fn passkey_register_and_sign_in() {
    let s = Server::new("passkey");
    let session = s.sign_in("gil@example.com");
    let origin = Url::parse(ORIGIN).unwrap();
    let mut authenticator = SoftPasskey::new(true);

    // Registration needs a session.
    assert_eq!(s.call("POST", "/v1/auth/passkey/register/start", None, None).status, 401);
    let start = s.call("POST", "/v1/auth/passkey/register/start", Some(&session), None);
    assert_eq!(start.status, 200, "{}", start.json);
    let options: CreationChallengeResponse = serde_json::from_value(start.json["options"].clone()).unwrap();
    let credential = authenticator.do_registration(origin.clone(), options).expect("soft passkey registration");
    let finish = s.call(
        "POST",
        "/v1/auth/passkey/register/finish",
        Some(&session),
        Some(json!({ "challengeId": start.json["challengeId"], "name": "test key", "credential": credential })),
    );
    assert_eq!(finish.status, 201, "{}", finish.json);
    // A challenge works once.
    let replay = s.call("POST", "/v1/auth/passkey/register/finish", Some(&session), Some(json!({ "challengeId": start.json["challengeId"], "credential": credential })));
    assert_eq!(replay.status, 400);
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["passkeys"][0]["name"], "test key");

    // Sign in with it, no email round trip.
    let start = s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "gil@example.com" })));
    assert_eq!(start.status, 200, "{}", start.json);
    let options: RequestChallengeResponse = serde_json::from_value(start.json["options"].clone()).unwrap();
    let assertion = authenticator.do_authentication(origin.clone(), options).expect("soft passkey assertion");
    let login = s.call("POST", "/v1/auth/passkey/login/finish", None, Some(json!({ "challengeId": start.json["challengeId"], "credential": assertion })));
    assert_eq!(login.status, 200, "{}", login.json);
    let new_session = login.json["session"].as_str().unwrap();
    assert_eq!(s.call("GET", "/v1/me", Some(new_session), None).json["email"], "gil@example.com");

    // The same assertion can't be replayed against a new challenge.
    let start = s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "gil@example.com" })));
    let replay = s.call("POST", "/v1/auth/passkey/login/finish", None, Some(json!({ "challengeId": start.json["challengeId"], "credential": assertion })));
    assert_eq!(replay.status, 401);

    // An assertion made for another origin is refused.
    let start = s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "gil@example.com" })));
    let options: RequestChallengeResponse = serde_json::from_value(start.json["options"].clone()).unwrap();
    let phished = authenticator.do_authentication(Url::parse("https://evil.example").unwrap(), options);
    if let Ok(phished) = phished {
        let r = s.call("POST", "/v1/auth/passkey/login/finish", None, Some(json!({ "challengeId": start.json["challengeId"], "credential": phished })));
        assert_eq!(r.status, 401);
    }

    // No passkey, or no account: no challenge.
    s.sign_in("hal@example.com");
    assert_eq!(s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "hal@example.com" }))).status, 404);
    assert_eq!(s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "nobody@example.com" }))).status, 404);
}

#[test]
fn serves_no_files_and_answers_json() {
    let s = Server::new("files");
    std::fs::write("hitop-sync.db", b"must not be served").ok();
    for path in ["/hitop-sync.db", "/sync.db", "/Cargo.toml", "/../Cargo.toml", "/src/main.rs"] {
        let r = s.call("GET", path, None, None);
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.json["error"], "not_found", "{path}");
    }
    std::fs::remove_file("hitop-sync.db").ok();
    let r = s.call("GET", "/", None, None);
    assert_eq!(r.json["license"], "AGPL-3.0-or-later");
    let health = s.call("GET", "/healthz", None, None).json;
    assert_eq!((health["ok"].as_bool(), health["database"].as_str()), (Some(true), Some("sqlite")));
    assert_eq!(health["schema"], hitop_sync_server::Db::schema_version() as i64);
    let page = s.call("GET", "/signin", None, None);
    assert_eq!(page.status, 200);
    assert!(String::from_utf8_lossy(&page.raw).contains("location.hash"));
    assert!(page.header("content-security-policy").unwrap().contains("default-src 'none'"));
}

#[test]
fn cors_only_for_allowed_origins() {
    let s = Server::new("cors");
    let r = s.raw("OPTIONS", "/v1/me", None, vec![], &[("Origin", "https://app.test.example")]);
    assert_eq!(r.status, 204);
    assert_eq!(r.header("access-control-allow-origin"), Some("https://app.test.example"));
    assert!(r.header("access-control-allow-headers").unwrap().contains("Authorization"));
    let r = s.raw("GET", "/", None, vec![], &[("Origin", "https://evil.example")]);
    assert_eq!(r.header("access-control-allow-origin"), None);
    assert_eq!(r.header("cache-control"), Some("no-store"));
}
