//! "Continue with Google" (OpenID Connect, against tests/common/idp.rs as Google) and "Continue
//! with GitHub" (OAuth 2, against tests/common/github.rs): sign-up, linking by verified address
//! only, refusals, and the same one-time code handoff as SSO.

mod common;

use common::github::{self as gh, MockGithub};
use common::idp::{self, MockIdp};
use common::*;
use serde_json::{json, Value};
use sha2::Digest;

struct Flow {
    verifier: String,
    state: String,
    nonce: String,
    pkce: String,
    url: url::Url,
}

fn start(s: &Server, kind: &str, return_to: Option<&str>) -> Flow {
    let verifier = b64(uuid::Uuid::new_v4().as_bytes()) + &b64(uuid::Uuid::new_v4().as_bytes());
    let challenge = b64(&sha2::Sha256::digest(verifier.as_bytes()));
    let mut body = json!({ "challenge": challenge });
    if let Some(r) = return_to {
        body["returnTo"] = json!(r);
    }
    let r = s.call("POST", &format!("/v1/auth/{kind}/start"), None, Some(body));
    assert_eq!(r.status, 200, "{}", r.json);
    let url = url::Url::parse(r.json["url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["redirect_uri"], format!("{ORIGIN}/v1/auth/{kind}/callback"));
    assert_eq!(q["code_challenge_method"], "S256");
    Flow { verifier, state: q["state"].clone(), nonce: q.get("nonce").cloned().unwrap_or_default(), pkce: q["code_challenge"].clone(), url }
}

fn callback(s: &Server, kind: &str, code: &str, flow: &Flow) -> Reply {
    s.raw("GET", &format!("/v1/auth/{kind}/callback?code={code}&state={}", flow.state), None, vec![], &[])
}

fn finish(s: &Server, kind: &str, reply: &Reply, flow: &Flow) -> Reply {
    let code = sso_code(reply).unwrap_or_else(|| panic!("no code: {} {}", reply.status, String::from_utf8_lossy(&reply.raw)));
    s.call("POST", &format!("/v1/auth/{kind}/finish"), None, Some(json!({ "code": code, "verifier": flow.verifier })))
}

fn last_failure(s: &Server) -> Value {
    audit_rows(s, "").into_iter().rev().find(|r| r["event"] == "signin_failed").expect("a signin_failed row")["detail"].clone()
}

fn google_server(name: &str, idp: &MockIdp, tweak: impl FnOnce(&mut hitop_sync_server::Config)) -> Server {
    let mut c = idp.config();
    c.name = "Google".into();
    c.scopes = "openid email".into();
    Server::with(name, |config| {
        config.google = Some(c);
        tweak(config);
    })
}

#[test]
fn off_unless_configured() {
    let s = Server::new("oauth-off");
    for kind in ["google", "github"] {
        let r = s.call("POST", &format!("/v1/auth/{kind}/start"), None, Some(json!({ "challenge": b64(&[0; 32]) })));
        assert_eq!((r.status, r.json["error"].as_str()), (404, Some("provider_off")), "{kind}");
        assert_eq!(s.call("POST", &format!("/v1/auth/{kind}/finish"), None, Some(json!({ "code": "x", "verifier": "y" }))).status, 404);
        assert_eq!(s.raw("GET", &format!("/v1/auth/{kind}/callback?code=x&state=y"), None, vec![], &[]).status, 400);
    }
}

#[test]
fn google_signs_up_links_by_verified_address_and_hands_the_code_to_the_starter() {
    let idp = MockIdp::start();
    let s = google_server("oauth-google", &idp, |_| {});
    assert_eq!(s.call("GET", "/v1/auth/methods", None, None).json["google"], true);

    // A new address: an account is made.
    let f = start(&s, "google", Some("http://127.0.0.1:45678/done"));
    assert_eq!(f.url.query_pairs().find(|(k, _)| k == "scope").unwrap().1, "openid email");
    idp.next(idp.sign(&idp.claims("g-1", "New@Gmail.test", &f.nonce)), &f.pkce);
    let r = callback(&s, "google", idp::GOOD_CODE, &f);
    assert!(r.header("location").unwrap().starts_with("http://127.0.0.1:45678/done?sso="));
    // Someone else with the code but not the verifier gets nothing.
    let stolen = s.call("POST", "/v1/auth/google/finish", None, Some(json!({ "code": sso_code(&r).unwrap(), "verifier": "not-the-verifier" })));
    assert_eq!(stolen.status, 401);
    let f = start(&s, "google", None);
    idp.next(idp.sign(&idp.claims("g-1", "new@gmail.test", &f.nonce)), &f.pkce);
    let page = callback(&s, "google", idp::GOOD_CODE, &f);
    assert!(String::from_utf8_lossy(&page.raw).contains("Signed in with Google"));
    let ok = finish(&s, "google", &page, &f);
    assert_eq!(ok.status, 200, "{}", ok.json);
    assert_eq!(ok.json["user"]["email"], "new@gmail.test");
    let first = ok.json["user"]["id"].clone();

    // An existing account (made by mail code) is linked: Google verified the address.
    let session = s.sign_in("old@gmail.test");
    let old_id = s.call("GET", "/v1/me", Some(&session), None).json["id"].clone();
    let f = start(&s, "google", None);
    idp.next(idp.sign(&idp.claims("g-2", "old@gmail.test", &f.nonce)), &f.pkce);
    let r = finish(&s, "google", &callback(&s, "google", idp::GOOD_CODE, &f), &f);
    assert_eq!(r.json["user"]["id"], old_id);
    // ...and later found by subject even after the address changes at Google.
    let f = start(&s, "google", None);
    idp.next(idp.sign(&idp.claims("g-1", "renamed@gmail.test", &f.nonce)), &f.pkce);
    assert_eq!(finish(&s, "google", &callback(&s, "google", idp::GOOD_CODE, &f), &f).json["user"]["id"], first);

    // Unverified: refused, never linked.
    let f = start(&s, "google", None);
    let mut claims = idp.claims("g-3", "old@gmail.test", &f.nonce);
    claims["email_verified"] = json!(false);
    idp.next(idp.sign(&claims), &f.pkce);
    let r = callback(&s, "google", idp::GOOD_CODE, &f);
    assert_eq!(r.status, 400);
    assert_eq!(last_failure(&s), json!({ "method": "google", "reason": "unverified_email" }));
    let rows = s.app.state().db.tx(|tx| tx.query("SELECT subject FROM oidc_identities ORDER BY subject", &[])).unwrap();
    assert_eq!(rows.iter().map(|r| r.text(0)).collect::<Vec<_>>(), vec!["g-1", "g-2"]);
    // A token signed for another client.
    let f = start(&s, "google", None);
    let mut claims = idp.claims("g-4", "x@gmail.test", &f.nonce);
    claims["aud"] = json!("someone-else");
    idp.next(idp.sign(&claims), &f.pkce);
    assert_eq!(callback(&s, "google", idp::GOOD_CODE, &f).status, 400);
    // The audit log names the method.
    let signins: Vec<Value> = audit_rows(&s, "").into_iter().filter(|r| r["event"] == "signin").map(|r| r["detail"]["method"].clone()).collect();
    assert!(signins.contains(&json!("google")));
}

#[test]
fn google_and_github_never_get_around_sso_only_domains() {
    let idp = MockIdp::start();
    let corp = MockIdp::start();
    let gh = MockGithub::start();
    let mut sso = corp.config();
    sso.allowed_domains = vec!["corp.test".into()];
    sso.sso_required = true;
    let s = google_server("oauth-ssoonly", &idp, |c| {
        c.oidc = Some(sso);
        c.github = Some(gh.config());
    });
    let f = start(&s, "google", None);
    idp.next(idp.sign(&idp.claims("g-9", "a@corp.test", &f.nonce)), &f.pkce);
    assert_eq!(callback(&s, "google", idp::GOOD_CODE, &f).status, 400);
    assert_eq!(last_failure(&s)["reason"], "sso_required");
    let f = start(&s, "github", None);
    gh.next(9, json!([{ "email": "a@corp.test", "primary": true, "verified": true }]), &f.pkce);
    assert_eq!(callback(&s, "github", gh::GOOD_CODE, &f).status, 400);
    assert_eq!(last_failure(&s)["reason"], "sso_required");
}

#[test]
fn github_takes_only_the_primary_verified_address() {
    let gh = MockGithub::start();
    let s = Server::with("oauth-github", |c| c.github = Some(gh.config()));
    assert_eq!(s.call("GET", "/v1/auth/methods", None, None).json["github"], true);

    let f = start(&s, "github", Some("http://localhost:45679/"));
    let q: std::collections::HashMap<String, String> = f.url.query_pairs().into_owned().collect();
    assert_eq!(f.url.path(), "/login/oauth/authorize");
    assert_eq!(q["scope"], "read:user user:email");
    assert_eq!(q["client_id"], gh::CLIENT_ID);
    gh.next(1001, json!([{ "email": "second@x.test", "primary": false, "verified": true }, { "email": "Main@X.test", "primary": true, "verified": true }]), &f.pkce);
    let r = callback(&s, "github", gh::GOOD_CODE, &f);
    assert!(r.header("location").unwrap().starts_with("http://localhost:45679/?sso="), "{:?}", r.header("location"));
    let ok = finish(&s, "github", &r, &f);
    assert_eq!(ok.status, 200, "{}", ok.json);
    assert_eq!(ok.json["user"]["email"], "main@x.test");
    let row = s.app.state().db.tx(|tx| tx.query_one("SELECT issuer, subject FROM oidc_identities", &[])).unwrap().unwrap();
    assert_eq!((row.text(0), row.text(1)), ("https://github.com".to_string(), "1001".to_string()));

    // Links an existing account by the verified primary address.
    let session = s.sign_in("ada@x.test");
    let id = s.call("GET", "/v1/me", Some(&session), None).json["id"].clone();
    let f = start(&s, "github", None);
    gh.next(1002, json!([{ "email": "ada@x.test", "primary": true, "verified": true }]), &f.pkce);
    assert_eq!(finish(&s, "github", &callback(&s, "github", gh::GOOD_CODE, &f), &f).json["user"]["id"], id);

    // Refusals: an unverified primary (even if a verified secondary is the existing account's),
    // no primary, a bad code, a replayed state.
    let cases = [
        (json!([{ "email": "ada2@x.test", "primary": true, "verified": false }, { "email": "ada@x.test", "primary": false, "verified": true }]), gh::GOOD_CODE, "unverified_email"),
        (json!([{ "email": "z@x.test", "primary": false, "verified": true }]), gh::GOOD_CODE, "no_email"),
        (json!([{ "email": "z@x.test", "primary": true, "verified": true }]), "wrong-code", "code_refused"),
    ];
    for (emails, code, reason) in cases {
        let f = start(&s, "github", None);
        gh.next(1003, emails, &f.pkce);
        let r = callback(&s, "github", code, &f);
        assert_eq!(r.status, 400, "{reason}");
        assert_eq!(last_failure(&s), json!({ "method": "github", "reason": reason }));
        let again = callback(&s, "github", gh::GOOD_CODE, &f);
        assert_eq!(again.status, 400);
        assert_eq!(last_failure(&s)["reason"], "unknown_state");
    }
    // A wrong PKCE verifier at the token endpoint (another flow's challenge).
    let f = start(&s, "github", None);
    gh.next(1004, json!([{ "email": "p@x.test", "primary": true, "verified": true }]), "not-this-flows-challenge");
    assert_eq!(callback(&s, "github", gh::GOOD_CODE, &f).status, 400);
    assert_eq!(s.app.state().db.tx(|tx| tx.query("SELECT subject FROM oidc_identities", &[])).unwrap().len(), 2);
}

#[test]
fn provider_settings_from_the_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string());
    assert!(hitop_sync_server::oidc::google_from_env(&env(&[])).unwrap().is_none());
    let g = hitop_sync_server::oidc::google_from_env(&env(&[("HITOP_SYNC_GOOGLE_CLIENT_ID", "id"), ("HITOP_SYNC_GOOGLE_CLIENT_SECRET", "s")])).unwrap().unwrap();
    assert_eq!(g.issuer, "https://accounts.google.com");
    assert!(hitop_sync_server::oidc::google_from_env(&env(&[("HITOP_SYNC_GOOGLE_CLIENT_ID", "id")])).is_err(), "half a pair");
    let gh = hitop_sync_server::github::GithubConfig::from_env(&env(&[("HITOP_SYNC_GITHUB_CLIENT_ID", "id"), ("HITOP_SYNC_GITHUB_CLIENT_SECRET", "s")])).unwrap().unwrap();
    assert_eq!((gh.web_url.as_str(), gh.api_url.as_str()), ("https://github.com", "https://api.github.com"));
    assert!(hitop_sync_server::github::GithubConfig::from_env(&env(&[("HITOP_SYNC_GITHUB_CLIENT_ID", "id"), ("HITOP_SYNC_GITHUB_CLIENT_SECRET", "s"), ("HITOP_SYNC_GITHUB_URL", "http://evil.example")])).is_err());
}
