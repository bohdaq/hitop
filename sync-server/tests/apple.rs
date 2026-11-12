//! "Continue with Apple" against tests/common/idp.rs in Apple mode: the signed client secret
//! (client_secret_post, no PKCE), the form POST callback, linking, and Apple's server-to-server
//! notifications.

mod common;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use common::idp::{self, Key, MockIdp};
use common::*;
use hitop_sync_server::apple::AppleConfig;
use serde_json::{json, Value};
use sha2::Digest;

const TEAM: &str = "TEAM123456";
const KEY_ID: &str = "KEY1234567";
const APP_ID: &str = "tech.example.app";

struct Flow {
    verifier: String,
    state: String,
    nonce: String,
    url: url::Url,
}

fn pem(key: &Key) -> String {
    let b64 = STANDARD.encode(&key.pkcs8);
    let lines: Vec<&str> = b64.as_bytes().chunks(64).map(|c| std::str::from_utf8(c).unwrap()).collect();
    format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", lines.join("\n"))
}

/// A server with Apple pointed at the mock, and the mock checking the server's client secret.
fn apple_server(name: &str, idp: &MockIdp) -> Server {
    let key = Key::generate(KEY_ID);
    idp.inner.lock().unwrap().apple = Some((KEY_ID.into(), key.public.clone(), TEAM.into()));
    let config = AppleConfig::new(&idp.issuer, idp::CLIENT_ID, TEAM, KEY_ID, &pem(&key), vec![APP_ID.into()]);
    config.check().unwrap();
    Server::with(name, |c| c.apple = Some(config))
}

fn start(s: &Server, return_to: Option<&str>) -> Flow {
    let verifier = b64(uuid::Uuid::new_v4().as_bytes()) + &b64(uuid::Uuid::new_v4().as_bytes());
    let challenge = b64(&sha2::Sha256::digest(verifier.as_bytes()));
    let mut body = json!({ "challenge": challenge });
    if let Some(r) = return_to {
        body["returnTo"] = json!(r);
    }
    let r = s.call("POST", "/v1/auth/apple/start", None, Some(body));
    assert_eq!(r.status, 200, "{}", r.json);
    let url = url::Url::parse(r.json["url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["redirect_uri"], format!("{ORIGIN}/v1/auth/apple/callback"));
    Flow { verifier, state: q["state"].clone(), nonce: q["nonce"].clone(), url }
}

/// Apple's form POST to the callback.
fn callback(s: &Server, fields: &[(&str, &str)]) -> Reply {
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    s.raw("POST", "/v1/auth/apple/callback", None, body.into_bytes(), &[("Content-Type", "application/x-www-form-urlencoded")])
}

fn sign_in(s: &Server, idp: &MockIdp, claims: Value, f: &Flow) -> Reply {
    idp.next(idp.sign(&claims), "");
    let r = callback(s, &[("code", idp::GOOD_CODE), ("state", &f.state), ("user", r#"{"name":{"firstName":"Ada","lastName":"L"},"email":"x"}"#)]);
    let code = sso_code(&r).unwrap_or_else(|| panic!("no code: {} {}", r.status, String::from_utf8_lossy(&r.raw)));
    s.call("POST", "/v1/auth/apple/finish", None, Some(json!({ "code": code, "verifier": f.verifier })))
}

/// Apple's claims: `email_verified` and `is_private_email` as strings.
fn claims(idp: &MockIdp, sub: &str, email: &str, nonce: &str) -> Value {
    let mut c = idp.claims(sub, email, nonce);
    c["email_verified"] = json!("true");
    c["is_private_email"] = json!(email.ends_with("privaterelay.appleid.com").to_string());
    c
}

fn last_failure(s: &Server) -> Value {
    audit_rows(s, "").into_iter().rev().find(|r| r["event"] == "signin_failed").expect("a signin_failed row")["detail"].clone()
}

fn identities(s: &Server) -> Vec<String> {
    s.app.state().db.tx(|tx| tx.query("SELECT subject FROM oidc_identities ORDER BY subject", &[])).unwrap().iter().map(|r| r.text(0)).collect()
}

#[test]
fn off_unless_configured() {
    let s = Server::new("apple-off");
    assert_eq!(s.call("GET", "/v1/auth/methods", None, None).json["apple"], false);
    let r = s.call("POST", "/v1/auth/apple/start", None, Some(json!({ "challenge": b64(&[0; 32]) })));
    assert_eq!((r.status, r.json["error"].as_str()), (404, Some("provider_off")));
    assert_eq!(s.call("POST", "/v1/auth/apple/finish", None, Some(json!({ "code": "x", "verifier": "y" }))).status, 404);
    assert_eq!(callback(&s, &[("code", "x"), ("state", "y")]).status, 400);
    assert_eq!(s.call("POST", "/v1/auth/apple/notifications", None, Some(json!({ "payload": "x" }))).status, 404);
}

#[test]
fn settings_come_together_and_the_key_must_sign() {
    let key = pem(&Key::generate("k"));
    let env = |pairs: Vec<(&'static str, String)>| move |name: &str| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone());
    assert!(AppleConfig::from_env(&env(vec![])).unwrap().is_none());
    let partial = AppleConfig::from_env(&env(vec![("HITOP_SYNC_APPLE_CLIENT_ID", "tech.x.signin".into()), ("HITOP_SYNC_APPLE_TEAM_ID", TEAM.into())]));
    assert!(partial.unwrap_err().contains("together"));
    let all = |key: String| {
        vec![
            ("HITOP_SYNC_APPLE_CLIENT_ID", "tech.x.signin".to_string()),
            ("HITOP_SYNC_APPLE_TEAM_ID", TEAM.to_string()),
            ("HITOP_SYNC_APPLE_KEY_ID", KEY_ID.to_string()),
            ("HITOP_SYNC_APPLE_PRIVATE_KEY", key),
            ("HITOP_SYNC_APPLE_APP_IDS", "tech.x, tech.y".to_string()),
        ]
    };
    let c = AppleConfig::from_env(&env(all(key))).unwrap().unwrap();
    assert_eq!((c.oidc.issuer.as_str(), c.oidc.client_id.as_str(), c.oidc.scopes.as_str()), ("https://appleid.apple.com", "tech.x.signin", "openid email"));
    assert_eq!(c.app_ids, vec!["tech.x", "tech.y"]);
    assert!(!format!("{c:?}").contains("PRIVATE KEY"));
    let bad = AppleConfig::from_env(&env(all("-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----\n".into())));
    assert!(bad.unwrap_err().contains("P-256"));
}

#[test]
fn signs_up_with_a_form_post_and_a_signed_client_secret() {
    let idp = MockIdp::start();
    let s = apple_server("apple-signup", &idp);
    assert_eq!(s.call("GET", "/v1/auth/methods", None, None).json["apple"], true);

    let f = start(&s, Some("https://app.test.example/sync-callback.html"));
    let q: std::collections::HashMap<String, String> = f.url.query_pairs().into_owned().collect();
    assert_eq!((q["response_mode"].as_str(), q["scope"].as_str(), q["client_id"].as_str()), ("form_post", "openid email", idp::CLIENT_ID));
    assert!(!q.contains_key("code_challenge"), "Apple gets no PKCE");

    idp.next(idp.sign(&claims(&idp, "a-1", "abc123@privaterelay.appleid.com", &f.nonce)), "");
    let r = callback(&s, &[("code", idp::GOOD_CODE), ("state", &f.state), ("user", r#"{"name":{"firstName":"Ada"}}"#)]);
    assert!(r.header("location").unwrap().starts_with("https://app.test.example/sync-callback.html#sso="), "{:?} {}", r.header("location"), String::from_utf8_lossy(&r.raw));
    let form = idp.inner.lock().unwrap().last_form.clone();
    assert_eq!(form["redirect_uri"], format!("{ORIGIN}/v1/auth/apple/callback"));
    let ok = s.call("POST", "/v1/auth/apple/finish", None, Some(json!({ "code": sso_code(&r).unwrap(), "verifier": f.verifier })));
    assert_eq!(ok.status, 200, "{}", ok.json);
    assert_eq!(ok.json["user"]["email"], "abc123@privaterelay.appleid.com");
    let relay = ok.json["user"]["id"].clone();
    let signins: Vec<Value> = audit_rows(&s, "").into_iter().filter(|r| r["event"] == "signin").map(|r| r["detail"]["method"].clone()).collect();
    assert!(signins.contains(&json!("apple")));

    // A shared (verified) address links the existing account.
    let session = s.sign_in("ada@example.test");
    let id = s.call("GET", "/v1/me", Some(&session), None).json["id"].clone();
    let f = start(&s, None);
    assert_eq!(sign_in(&s, &idp, claims(&idp, "a-2", "Ada@Example.test", &f.nonce), &f).json["user"]["id"], id);
    // The same Apple user again: found by subject, the secret reused from the cache.
    let f = start(&s, None);
    assert_eq!(sign_in(&s, &idp, claims(&idp, "a-1", "abc123@privaterelay.appleid.com", &f.nonce), &f).json["user"]["id"], relay);
    assert_eq!(identities(&s), vec!["a-1", "a-2"]);
}

#[test]
fn refusals() {
    let idp = MockIdp::start();
    let s = apple_server("apple-refusals", &idp);

    // The person cancels at Apple.
    let f = start(&s, None);
    let r = callback(&s, &[("error", "user_cancelled_authorize"), ("state", &f.state)]);
    assert_eq!(r.status, 400);
    assert_eq!(last_failure(&s), json!({ "method": "apple", "reason": "provider_error" }));
    // A replayed or unknown state.
    assert_eq!(callback(&s, &[("code", idp::GOOD_CODE), ("state", &f.state)]).status, 400);
    assert_eq!(last_failure(&s)["reason"], "unknown_state");
    // A query string instead of a form: no state in the body.
    assert_eq!(s.raw("POST", &format!("/v1/auth/apple/callback?code={}&state={}", idp::GOOD_CODE, f.state), None, vec![], &[]).status, 400);
    // A wrong nonce.
    let f = start(&s, None);
    idp.next(idp.sign(&claims(&idp, "a-9", "n@example.test", "another-nonce")), "");
    assert_eq!(callback(&s, &[("code", idp::GOOD_CODE), ("state", &f.state)]).status, 400);
    assert_eq!(last_failure(&s)["reason"], "bad_nonce");
    // Apple refuses the client secret (another team's key).
    let other = Key::generate(KEY_ID);
    idp.inner.lock().unwrap().apple = Some((KEY_ID.into(), other.public.clone(), TEAM.into()));
    let f = start(&s, None);
    idp.next(idp.sign(&claims(&idp, "a-9", "n@example.test", &f.nonce)), "");
    assert_eq!(callback(&s, &[("code", idp::GOOD_CODE), ("state", &f.state)]).status, 400);
    assert_eq!(last_failure(&s)["reason"], "code_refused");
    // A body too large.
    let big = "x".repeat(70 * 1024);
    assert_eq!(callback(&s, &[("code", &big), ("state", "s")]).status, 400);
    assert!(identities(&s).is_empty());
}

#[test]
fn notifications_unlink_on_revoke_and_check_the_signature() {
    let idp = MockIdp::start();
    let s = apple_server("apple-notify", &idp);
    let f = start(&s, None);
    let user = sign_in(&s, &idp, claims(&idp, "a-1", "ada@example.test", &f.nonce), &f).json["user"]["id"].clone();
    let f = start(&s, None);
    sign_in(&s, &idp, claims(&idp, "a-2", "bob@example.test", &f.nonce), &f);

    let payload = |aud: &str, kind: &str, sub: &str| {
        let now = hitop_sync_server::api::now_secs();
        let events = json!({ "type": kind, "sub": sub, "event_time": now * 1000 }).to_string();
        json!({ "payload": idp.sign(&json!({ "iss": idp.issuer, "aud": aud, "iat": now, "jti": "j", "events": events })) })
    };
    let notify = |body: Value| s.call("POST", "/v1/auth/apple/notifications", None, Some(body));

    // Signed by someone else, for another app, or not a JWT: refused, nothing changes.
    let stranger = MockIdp::start();
    let forged = json!({ "payload": stranger.sign(&json!({ "iss": idp.issuer, "aud": idp::CLIENT_ID, "iat": 1, "events": json!({ "type": "consent-revoked", "sub": "a-1" }).to_string() })) });
    assert_eq!(notify(forged).status, 401);
    assert_eq!(notify(payload("tech.other.app", "consent-revoked", "a-1")).status, 401);
    assert_eq!(notify(json!({ "payload": "not-a-jwt" })).status, 401);
    assert_eq!(identities(&s), vec!["a-1", "a-2"]);

    // Mail forwarding changes are recorded; the link stays.
    assert_eq!(notify(payload(idp::CLIENT_ID, "email-disabled", "a-2")).status, 200);
    // Consent revoked (to the Services ID) and account deleted (to the App ID): unlinked.
    assert_eq!(notify(payload(idp::CLIENT_ID, "consent-revoked", "a-1")).status, 200);
    assert_eq!(notify(payload(APP_ID, "account-delete", "a-2")).status, 200);
    assert!(identities(&s).is_empty());
    // An unknown subject is fine (already unlinked).
    assert_eq!(notify(payload(idp::CLIENT_ID, "consent-revoked", "a-1")).status, 200);

    let rows = audit_rows(&s, "");
    let unlinked: Vec<Value> = rows.iter().filter(|r| r["event"] == "sso_unlinked").map(|r| json!([r["target"], r["detail"]["reason"]])).collect();
    assert_eq!(unlinked.len(), 2);
    assert_eq!(unlinked[0], json!([user, "consent-revoked"]));
    assert!(rows.iter().any(|r| r["event"] == "email_forwarding_changed" && r["detail"]["forwarding"] == false));
    // The account itself stays.
    assert!(s.app.state().db.tx(|tx| tx.query_one("SELECT id FROM users WHERE id = $1", &[user.as_str().unwrap().into()])).unwrap().is_some());
}
