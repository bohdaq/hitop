//! Single sign-on against a real Keycloak (`quay.io/keycloak/keycloak`, dev mode) with the realm
//! in tests/fixtures/keycloak/hitop-realm.json. Start it and point the tests at it:
//!
//! ```text
//! docker run -d --name hitop-keycloak -p 127.0.0.1:18080:8080 \
//!   -v "$PWD/tests/fixtures/keycloak:/opt/keycloak/data/import:ro" \
//!   quay.io/keycloak/keycloak:26.4.2 start-dev --import-realm
//! HITOP_SYNC_TEST_KEYCLOAK=http://127.0.0.1:18080 cargo test --test keycloak
//! ```
//!
//! Without the variable the tests are skipped, except in CI (`CI` set), where they fail.
//! A test signs in the way a browser does: it opens the authorization URL the server made,
//! posts the realm's login form with the user's password, and hands the provider's redirect
//! back to the server, in process.

mod common;

use std::collections::HashMap;

use common::*;
use hitop_sync_server::oidc::OidcConfig;
use serde_json::json;
use sha2::Digest;

const CLIENT_SECRET: &str = "hitop-sync-test-secret";

fn keycloak() -> Option<String> {
    match std::env::var("HITOP_SYNC_TEST_KEYCLOAK").ok().filter(|s| !s.is_empty()) {
        Some(url) => Some(url.trim_end_matches('/').to_string()),
        None if std::env::var_os("CI").is_some() => panic!("HITOP_SYNC_TEST_KEYCLOAK must be set in CI"),
        None => {
            eprintln!("skipped: HITOP_SYNC_TEST_KEYCLOAK is not set");
            None
        }
    }
}

fn config(base: &str) -> OidcConfig {
    OidcConfig {
        issuer: format!("{base}/realms/hitop"),
        client_id: "hitop-sync".into(),
        client_secret: CLIENT_SECRET.into(),
        scopes: "openid email profile".into(),
        name: "Keycloak".into(),
        allowed_domains: vec![],
        groups_claim: "groups".into(),
        required_groups: vec![],
        sso_required: false,
        assume_verified: false,
    }
}

fn server(name: &str, base: &str, tweak: impl FnOnce(&mut OidcConfig)) -> Server {
    let mut c = config(base);
    tweak(&mut c);
    Server::with(name, |config| config.oidc = Some(c))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().max_redirects(0).http_status_as_error(false).build().into()
}

fn cookies(res: &ureq::http::Response<ureq::Body>, jar: &mut HashMap<String, String>) {
    for v in res.headers().get_all("set-cookie") {
        let pair = v.to_str().unwrap().split(';').next().unwrap();
        if let Some((k, v)) = pair.split_once('=') {
            jar.insert(k.to_string(), v.to_string());
        }
    }
}

fn cookie_header(jar: &HashMap<String, String>) -> String {
    jar.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ")
}

/// Signs `username` in at Keycloak's login page for `authorize`; returns Keycloak's redirect
/// (the callback URL with `code` and `state`).
fn browser_login(authorize: &str, username: &str, password: &str) -> url::Url {
    let http = agent();
    let mut jar = HashMap::new();
    let mut res = http.get(authorize).call().unwrap();
    cookies(&res, &mut jar);
    let page = res.body_mut().read_to_string().unwrap();
    assert_eq!(res.status().as_u16(), 200, "{page}");
    let form = &page[page.find("id=\"kc-form-login\"").expect("Keycloak's login form")..];
    let action = &form[form.find("action=\"").unwrap() + 8..];
    let action = action[..action.find('"').unwrap()].replace("&amp;", "&");
    let res = http.post(&action).header("Cookie", &cookie_header(&jar)).send_form([("username", username), ("password", password), ("credentialId", "")]).unwrap();
    assert_eq!(res.status().as_u16(), 302, "Keycloak didn't accept the login of {username}");
    url::Url::parse(res.headers().get("location").unwrap().to_str().unwrap()).unwrap()
}

/// The provider's redirect, handed to the server as the browser would.
fn back_to_server(s: &Server, redirect: &url::Url) -> Reply {
    assert!(redirect.as_str().starts_with(&format!("{ORIGIN}/v1/auth/sso/callback?")), "{redirect}");
    sso_callback(s, redirect.query().unwrap())
}

/// A full SSO sign-in; `Err` holds the refusal page's text.
fn sso(s: &Server, username: &str, password: &str) -> Result<String, String> {
    let flow = sso_start(s, Some("http://127.0.0.1:45679/"));
    let reply = back_to_server(s, &browser_login(flow.url.as_str(), username, password));
    let Some(code) = sso_code(&reply) else {
        return Err(String::from_utf8_lossy(&reply.raw).into_owned());
    };
    let r = sso_finish(s, &code, &flow.verifier);
    assert_eq!(r.status, 200, "{}", r.json);
    Ok(r.json["session"].as_str().unwrap().to_string())
}

fn user_id(s: &Server, session: &str) -> String {
    s.call("GET", "/v1/me", Some(session), None).json["id"].as_str().unwrap().to_string()
}

#[test]
fn signs_in_and_links_to_the_existing_account() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-login", &kc, |_| {});
    // alice signed in by email before SSO existed.
    let mailed = s.sign_in("alice@corp.test");
    let alice = user_id(&s, &mailed);
    let session = sso(&s, "alice", "alice-test-pw").unwrap();
    assert_eq!(user_id(&s, &session), alice, "linked by verified address");
    // From now on by (issuer, subject).
    let again = sso(&s, "alice", "alice-test-pw").unwrap();
    assert_eq!(user_id(&s, &again), alice);
    let identities = s.app.state().db.tx(|tx| tx.query("SELECT issuer, user_id, email FROM oidc_identities", &[])).unwrap();
    assert_eq!(identities.len(), 1);
    assert_eq!((identities[0].text(0), identities[0].text(1)), (format!("{kc}/realms/hitop"), alice.clone()));
    // A new person gets a new account.
    let bob = sso(&s, "bob", "bob-test-pw").unwrap();
    assert_ne!(user_id(&s, &bob), alice);
    let events: Vec<String> = audit_rows(&s, "").iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert_eq!(events.iter().filter(|e| *e == "sso_linked").count(), 2, "{events:?}");
}

#[test]
fn never_links_an_unverified_address() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-unverified", &kc, |_| {});
    let mailed = s.sign_in("carol@corp.test");
    let err = sso(&s, "carol", "carol-test-pw").unwrap_err();
    assert!(err.contains("verified"), "{err}");
    let identities = s.app.state().db.tx(|tx| tx.query("SELECT user_id FROM oidc_identities", &[])).unwrap();
    assert!(identities.is_empty());
    assert_eq!(s.call("GET", "/v1/me", Some(&mailed), None).status, 200, "her email account is untouched");
}

#[test]
fn domain_and_group_restrictions() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-policy", &kc, |c| {
        c.allowed_domains = vec!["corp.test".into()];
        c.required_groups = vec!["hitop-users".into()];
    });
    assert!(sso(&s, "bob", "bob-test-pw").unwrap_err().contains("domain"), "bob@other.test");
    assert!(sso(&s, "dave", "dave-test-pw").unwrap_err().contains("group"), "dave isn't in hitop-users");
    sso(&s, "alice", "alice-test-pw").unwrap();
}

#[test]
fn sso_required_turns_off_mail_and_passkey_sign_in_for_its_domains() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-required", &kc, |c| {
        c.allowed_domains = vec!["corp.test".into()];
        c.sso_required = true;
    });
    let r = s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "alice@corp.test" })));
    assert_eq!((r.status, r.json["error"].as_str()), (403, Some("sso_required")));
    assert_eq!(s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "alice@corp.test" }))).status, 403);
    // Other domains still get mails (and can't use SSO here).
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "bob@other.test" }))).status, 202);
    let session = sso(&s, "alice", "alice-test-pw").unwrap();
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["email"], "alice@corp.test");
}

#[test]
fn replayed_codes_and_states_are_refused() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-replay", &kc, |_| {});
    let flow = sso_start(&s, None);
    let redirect = browser_login(flow.url.as_str(), "alice", "alice-test-pw");
    assert_eq!(back_to_server(&s, &redirect).status, 200);
    assert_eq!(back_to_server(&s, &redirect).status, 400, "the same redirect twice");
    // A code from one sign-in pushed into another: the provider refuses it (PKCE), and the
    // server would refuse its token anyway (nonce).
    let a = sso_start(&s, None);
    let stolen = browser_login(a.url.as_str(), "alice", "alice-test-pw");
    let code = stolen.query_pairs().find(|(k, _)| k == "code").unwrap().1.into_owned();
    let b = sso_start(&s, None);
    assert_eq!(sso_callback(&s, &format!("code={code}&state={}", b.state)).status, 400);
}

/// A token straight from Keycloak (the test is its own client, with its own nonce and PKCE).
fn keycloak_token(kc: &str, host: &str, nonce: &str) -> String {
    let verifier = "a-test-pkce-verifier-that-is-long-enough-0123456789";
    let challenge = b64(&sha2::Sha256::digest(verifier.as_bytes()));
    let redirect = format!("{ORIGIN}/v1/auth/sso/callback");
    let mut auth = url::Url::parse(&format!("{}/realms/hitop/protocol/openid-connect/auth", kc.replace("127.0.0.1", host))).unwrap();
    auth.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", "hitop-sync")
        .append_pair("redirect_uri", &redirect)
        .append_pair("scope", "openid email")
        .append_pair("state", "test")
        .append_pair("nonce", nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    let back = browser_login(auth.as_str(), "alice", "alice-test-pw");
    let code = back.query_pairs().find(|(k, _)| k == "code").unwrap().1.into_owned();
    let mut res = agent()
        .post(&format!("{}/realms/hitop/protocol/openid-connect/token", kc.replace("127.0.0.1", host)))
        .send_form([("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", &redirect), ("code_verifier", verifier), ("client_id", "hitop-sync"), ("client_secret", CLIENT_SECRET)])
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&res.body_mut().read_to_string().unwrap()).unwrap();
    body["id_token"].as_str().unwrap_or_else(|| panic!("{body}")).to_string()
}

#[test]
fn bad_keycloak_tokens_are_refused() {
    use hitop_sync_server::oidc::validate_id_token;
    let Some(kc) = keycloak() else { return };
    let s = server("kc-tokens", &kc, |_| {});
    let st = s.app.state();
    let token = keycloak_token(&kc, "127.0.0.1", "nonce-1");
    let ok = validate_id_token(st, &token, "nonce-1").unwrap();
    assert_eq!(ok.email, "alice@corp.test");
    // Nonce replay: a real token presented to another sign-in.
    assert_eq!(validate_id_token(st, &token, "nonce-2").unwrap_err().reason, "bad_nonce");
    // Bad signature.
    let (rest, sig) = token.rsplit_once('.').unwrap();
    let flipped: String = sig.chars().enumerate().map(|(i, c)| if i == 10 { if c == 'A' { 'B' } else { 'A' } } else { c }).collect();
    assert_eq!(validate_id_token(st, &format!("{rest}.{flipped}"), "nonce-1").unwrap_err().reason, "bad_token");
    // Payload swapped (claims of another token under this signature).
    let other = keycloak_token(&kc, "127.0.0.1", "nonce-3");
    let parts: Vec<&str> = token.split('.').collect();
    let other_parts: Vec<&str> = other.split('.').collect();
    assert_eq!(validate_id_token(st, &format!("{}.{}.{}", parts[0], other_parts[1], parts[2]), "nonce-3").unwrap_err().reason, "bad_token");
    // Wrong audience: a server registered as another client.
    let other_client = server("kc-aud", &kc, |c| c.client_id = "another-client".into());
    assert_eq!(validate_id_token(other_client.app.state(), &token, "nonce-1").unwrap_err().reason, "bad_token");
    // Wrong issuer: Keycloak names itself after the host it was asked on.
    let localhost = keycloak_token(&kc, "localhost", "nonce-4");
    assert_eq!(validate_id_token(st, &localhost, "nonce-4").unwrap_err().reason, "bad_token");
    // Expired: Keycloak's ID tokens live 5 minutes in this realm.
    s.advance(3600);
    assert_eq!(validate_id_token(st, &token, "nonce-1").unwrap_err().reason, "expired_token");
}

#[test]
fn a_wrong_issuer_setting_is_a_config_error() {
    let Some(kc) = keycloak() else { return };
    let s = server("kc-issuer", &kc, |c| c.issuer = format!("{kc}/realms/no-such-realm"));
    let r = s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[7; 32]) })));
    assert_eq!(r.status, 503);
    assert_eq!(audit_rows(&s, "").pop().unwrap()["event"], "sso_config_error");
    // A wrong client secret is caught at the token endpoint.
    let s = server("kc-secret", &kc, |c| c.client_secret = "wrong".into());
    let flow = sso_start(&s, None);
    let reply = back_to_server(&s, &browser_login(flow.url.as_str(), "alice", "alice-test-pw"));
    assert_eq!(reply.status, 400);
    let events: Vec<String> = audit_rows(&s, "").iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert!(events.contains(&"sso_config_error".to_string()) && events.contains(&"signin_failed".to_string()), "{events:?}");
}
