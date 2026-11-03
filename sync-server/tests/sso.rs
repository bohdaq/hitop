//! Single sign-on against an in-test OpenID Connect provider (tests/common/idp.rs): the flow,
//! the one-time code handoff, and every kind of bad token. tests/keycloak.rs runs the real flow
//! against Keycloak.

mod common;

use common::idp::{self, MockIdp};
use common::*;
use hitop_sync_server::oidc::OidcConfig;
use serde_json::{json, Value};

fn server(name: &str, idp: &MockIdp, tweak: impl FnOnce(&mut OidcConfig)) -> Server {
    let mut c = idp.config();
    tweak(&mut c);
    Server::with(name, |config| config.oidc = Some(c))
}

/// Start, provider answers with `token(flow)`, callback. Returns the flow and the callback reply.
fn run(s: &Server, idp: &MockIdp, return_to: Option<&str>, token: impl FnOnce(&SsoFlow) -> String) -> (SsoFlow, Reply) {
    let flow = sso_start(s, return_to);
    idp.next(token(&flow), &flow.pkce);
    let reply = sso_callback(s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state));
    (flow, reply)
}

fn sign_in(s: &Server, idp: &MockIdp, sub: &str, email: &str) -> String {
    let (flow, reply) = run(s, idp, Some("http://127.0.0.1:45678/done"), |f| idp.sign(&idp.claims(sub, email, &f.nonce)));
    let code = sso_code(&reply).unwrap_or_else(|| panic!("no code: {} {}", reply.status, String::from_utf8_lossy(&reply.raw)));
    let r = sso_finish(s, &code, &flow.verifier);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json["session"].as_str().unwrap().to_string()
}

fn both(s: &Server, idp: &MockIdp, secs: i64) {
    s.advance(secs);
    idp.advance(secs);
}

fn refused_reason(s: &Server) -> String {
    let rows = audit_rows(s, "");
    let last = rows.iter().rev().find(|r| r["event"] == "signin_failed").expect("a signin_failed row");
    last["detail"]["reason"].as_str().unwrap().to_string()
}

#[test]
fn sso_info_and_off_by_default() {
    let plain = Server::new("sso-off");
    assert_eq!(plain.call("GET", "/v1/auth/sso", None, None).json, json!({ "enabled": false }));
    assert_eq!(plain.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[0; 32]) }))).status, 404);
    let idp = MockIdp::start();
    let s = server("sso-info", &idp, |c| {
        c.allowed_domains = vec!["corp.test".into()];
        c.sso_required = true;
    });
    assert_eq!(s.call("GET", "/v1/auth/sso", None, None).json, json!({ "enabled": true, "name": "Mock", "required": true, "domains": ["corp.test"] }));
}

#[test]
fn signs_in_hands_the_code_to_the_starting_client_only_and_links_by_subject() {
    let idp = MockIdp::start();
    let s = server("sso-flow", &idp, |_| {});
    let (flow, reply) = run(&s, &idp, Some("http://127.0.0.1:45678/done"), |f| idp.sign(&idp.claims("sub-ann", "Ann@Corp.test", &f.nonce)));
    // A loopback return gets the code in the query.
    assert_eq!(reply.status, 302);
    assert!(reply.header("location").unwrap().starts_with("http://127.0.0.1:45678/done?sso="));
    let code = sso_code(&reply).unwrap();
    // The code alone isn't enough: the verifier of the client that started is needed.
    assert_eq!(sso_finish(&s, &code, "someone-elses-verifier").status, 401);
    // ... and a refused finish used it up.
    assert_eq!(sso_finish(&s, &code, &flow.verifier).status, 401);
    let session = sign_in(&s, &idp, "sub-ann", "ann@corp.test");
    let me = s.call("GET", "/v1/me", Some(&session), None);
    assert_eq!(me.json["email"], "ann@corp.test");
    // The same subject is the same account, even when the provider changes the address.
    let again = sign_in(&s, &idp, "sub-ann", "ann.new@corp.test");
    assert_eq!(s.call("GET", "/v1/me", Some(&again), None).json["id"], me.json["id"]);
    // Codes work once.
    let (flow, reply) = run(&s, &idp, Some("http://localhost:45678/"), |f| idp.sign(&idp.claims("sub-ann", "ann@corp.test", &f.nonce)));
    let code = sso_code(&reply).unwrap();
    assert_eq!(sso_finish(&s, &code, &flow.verifier).status, 200);
    assert_eq!(sso_finish(&s, &code, &flow.verifier).status, 401);
    // An existing magic-link account with a verified address is linked, not duplicated.
    let mailed = s.sign_in("bea@corp.test");
    let bea = s.call("GET", "/v1/me", Some(&mailed), None).json["id"].clone();
    let sso = sign_in(&s, &idp, "sub-bea", "bea@corp.test");
    assert_eq!(s.call("GET", "/v1/me", Some(&sso), None).json["id"], bea);
    let events: Vec<String> = audit_rows(&s, "").iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert_eq!(events.iter().filter(|e| *e == "sso_linked").count(), 2, "{events:?}");
    let last = audit_rows(&s, "").pop().unwrap();
    assert_eq!((last["event"].as_str(), last["detail"]["method"].as_str()), (Some("signin"), Some("sso")));
}

#[test]
fn the_code_goes_only_to_loopback_allowed_origins_or_a_page() {
    let idp = MockIdp::start();
    let s = server("sso-return", &idp, |_| {});
    for bad in ["https://evil.example/", "http://127.0.0.1/no-port", "http://192.168.1.2:8080/", "https://app.test.example.evil/", "http://u:p@127.0.0.1:1/", "https://app.test.example/#x"] {
        let r = s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[1; 32]), "returnTo": bad })));
        assert_eq!(r.status, 400, "{bad}");
    }
    assert_eq!(s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": "short" }))).status, 400);
    // A web app origin from HITOP_SYNC_ALLOWED_ORIGINS gets it in the fragment (never sent to a server).
    let (flow, reply) = run(&s, &idp, Some("https://app.test.example/sso-done.html"), |f| idp.sign(&idp.claims("s1", "cy@corp.test", &f.nonce)));
    assert_eq!(reply.status, 302);
    assert!(reply.header("location").unwrap().starts_with("https://app.test.example/sso-done.html#sso="));
    assert_eq!(sso_finish(&s, &sso_code(&reply).unwrap(), &flow.verifier).status, 200);
    // No return address: a page to paste the code from.
    let (flow, reply) = run(&s, &idp, None, |f| idp.sign(&idp.claims("s1", "cy@corp.test", &f.nonce)));
    assert_eq!(reply.status, 200);
    assert!(reply.header("content-security-policy").unwrap().contains("default-src 'none'"));
    assert!(String::from_utf8_lossy(&reply.raw).contains("cy@corp.test"));
    assert_eq!(sso_finish(&s, &sso_code(&reply).unwrap(), &flow.verifier).status, 200);
}

#[test]
fn bad_tokens_are_refused() {
    let idp = MockIdp::start();
    let s = server("sso-bad", &idp, |_| {});
    let now = hitop_sync_server::api::now_secs();
    let other_key = idp::Key::generate("k1");
    type Case = (&'static str, &'static str, Box<dyn Fn(&SsoFlow) -> String>);
    let i = idp.clone();
    let claims = move |f: &SsoFlow| i.claims("sub-x", "x@corp.test", &f.nonce);
    let c = claims.clone();
    let i = idp.clone();
    let with = move |f: &SsoFlow, k: &str, v: Value| {
        let mut cl = c(f);
        cl[k] = v;
        i.sign(&cl)
    };
    let w = with.clone();
    let cases: Vec<Case> = vec![
        ("wrong audience", "bad_token", Box::new(move |f| w(f, "aud", json!("another-client")))),
        ("wrong issuer", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "iss", json!("https://evil.example/realm")))
        }),
        ("expired", "expired_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "exp", json!(now - 3600)))
        }),
        ("issued in the future", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "iat", json!(now + 3600)))
        }),
        ("iat as a string", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "iat", json!((now + 3600).to_string())))
        }),
        ("not valid yet", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "nbf", json!(now + 3600)))
        }),
        ("nbf as a string", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "nbf", json!((now + 3600).to_string())))
        }),
        ("exp as a string", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "exp", json!((now + 3600).to_string())))
        }),
        ("nonce of another sign-in", "bad_nonce", {
            let w = with.clone();
            Box::new(move |f| w(f, "nonce", json!("replayed-nonce")))
        }),
        ("no nonce", "bad_nonce", {
            let c = claims.clone();
            let i = idp.clone();
            Box::new(move |f| {
                let mut cl = c(f);
                cl.as_object_mut().unwrap().remove("nonce");
                i.sign(&cl)
            })
        }),
        ("several audiences without azp", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "aud", json!([idp::CLIENT_ID, "other"])))
        }),
        ("azp of another client", "bad_token", {
            let w = with.clone();
            Box::new(move |f| w(f, "azp", json!("other")))
        }),
        ("signed by a key the provider never published", "bad_token", {
            let c = claims.clone();
            Box::new(move |f| idp::sign_with("k1", &other_key.pkcs8, &c(f)))
        }),
        ("alg none", "bad_token", {
            let c = claims.clone();
            Box::new(move |f| format!("{}.{}.", b64(br#"{"alg":"none","typ":"JWT"}"#), b64(c(f).to_string().as_bytes())))
        }),
        ("HS256 with the client secret", "bad_token", {
            let c = claims.clone();
            Box::new(move |f| {
                let h = jsonwebtoken::Header { kid: Some("k1".into()), ..jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256) };
                jsonwebtoken::encode(&h, &c(f), &jsonwebtoken::EncodingKey::from_secret(idp::CLIENT_SECRET.as_bytes())).unwrap()
            })
        }),
        ("garbage", "bad_token", Box::new(|_| "not.a.jwt".into())),
        ("unverified address", "unverified_email", {
            let w = with.clone();
            Box::new(move |f| w(f, "email_verified", json!(false)))
        }),
        ("no email_verified claim", "unverified_email", {
            let c = claims.clone();
            let i = idp.clone();
            Box::new(move |f| {
                let mut cl = c(f);
                cl.as_object_mut().unwrap().remove("email_verified");
                i.sign(&cl)
            })
        }),
        ("no email", "no_email", {
            let w = with.clone();
            Box::new(move |f| w(f, "email", json!("")))
        }),
        ("a runner address", "no_email", {
            let w = with.clone();
            Box::new(move |f| w(f, "email", json!("abc@runner.invalid")))
        }),
    ];
    for (name, reason, token) in cases {
        let (_, reply) = run(&s, &idp, Some("http://127.0.0.1:45678/"), token);
        assert_eq!(reply.status, 400, "{name}: {}", String::from_utf8_lossy(&reply.raw));
        assert!(sso_code(&reply).is_none(), "{name}");
        assert_eq!(refused_reason(&s), reason, "{name}");
    }
    // A tampered signature on an otherwise good token.
    let (_, reply) = run(&s, &idp, None, |f| {
        let t = idp.sign(&idp.claims("sub-x", "x@corp.test", &f.nonce));
        let (rest, sig) = t.rsplit_once('.').unwrap();
        let mut sig = sig.as_bytes().to_vec();
        sig[5] = if sig[5] == b'A' { b'B' } else { b'A' };
        format!("{rest}.{}", String::from_utf8(sig).unwrap())
    });
    assert_eq!(reply.status, 400);
    // A good token replayed into a new sign-in fails on its nonce.
    let (first, reply) = run(&s, &idp, None, |f| idp.sign(&idp.claims("sub-x", "x@corp.test", &f.nonce)));
    assert_eq!(reply.status, 200);
    let old = idp.sign(&idp.claims("sub-x", "x@corp.test", &first.nonce));
    let (_, reply) = run(&s, &idp, None, |_| old.clone());
    assert_eq!(reply.status, 400);
    assert_eq!(refused_reason(&s), "bad_nonce");
    // None of the refused ones made an account.
    let users = s.app.state().db.tx(|tx| tx.query("SELECT email FROM users", &[])).unwrap();
    assert_eq!(users.len(), 1, "only the good sign-in");
}

#[test]
fn state_is_single_use_and_flows_expire() {
    let idp = MockIdp::start();
    let s = server("sso-state", &idp, |_| {});
    let (flow, reply) = run(&s, &idp, None, |f| idp.sign(&idp.claims("s", "dee@corp.test", &f.nonce)));
    assert_eq!(reply.status, 200);
    // The provider's redirect replayed.
    let replay = sso_callback(&s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state));
    assert_eq!(replay.status, 400);
    assert_eq!(refused_reason(&s), "unknown_state");
    assert_eq!(sso_callback(&s, "code=x&state=made-up").status, 400);
    // The provider said no.
    let flow = sso_start(&s, None);
    assert_eq!(sso_callback(&s, &format!("error=access_denied&error_description=%3Cb%3Eno%3C%2Fb%3E&state={}", flow.state)).status, 400);
    assert_eq!(refused_reason(&s), "provider_error");
    // A wrong code (or PKCE verifier) at the token endpoint.
    let flow = sso_start(&s, None);
    idp.next("unused".into(), &flow.pkce);
    assert_eq!(sso_callback(&s, &format!("code=wrong&state={}", flow.state)).status, 400);
    assert_eq!(refused_reason(&s), "code_refused");
    // Ten minutes at most between start and callback.
    let flow = sso_start(&s, None);
    both(&s, &idp, 11 * 60);
    idp.next(idp.sign(&idp.claims("s", "dee@corp.test", &flow.nonce)), &flow.pkce);
    assert_eq!(sso_callback(&s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state)).status, 400);
    // And five minutes from callback to finish.
    let (flow, reply) = run(&s, &idp, None, |f| idp.sign(&idp.claims("s", "dee@corp.test", &f.nonce)));
    let code = sso_code(&reply).unwrap();
    both(&s, &idp, 6 * 60);
    assert_eq!(sso_finish(&s, &code, &flow.verifier).status, 401);
}

#[test]
fn keys_are_cached_and_fetched_again_when_the_provider_rotates() {
    let idp = MockIdp::start();
    let s = server("sso-jwks", &idp, |_| {});
    sign_in(&s, &idp, "a", "eve@corp.test");
    sign_in(&s, &idp, "a", "eve@corp.test");
    assert_eq!(idp.jwks_fetches(), 1, "the JWKS is cached");
    // A new key id: fetched again (after the one-minute floor), then cached.
    idp.rotate("k2");
    both(&s, &idp, 61);
    sign_in(&s, &idp, "a", "eve@corp.test");
    assert_eq!(idp.jwks_fetches(), 2);
    sign_in(&s, &idp, "a", "eve@corp.test");
    assert_eq!(idp.jwks_fetches(), 2);
    // Unknown key ids can't make the server hammer the provider: one fetch a minute at most.
    idp.rotate("k3");
    let (_, reply) = run(&s, &idp, None, |f| idp.sign(&idp.claims("a", "eve@corp.test", &f.nonce)));
    assert_eq!(reply.status, 400, "k3 is unknown and the JWKS was fetched under a minute ago");
    assert_eq!(idp.jwks_fetches(), 2);
    both(&s, &idp, 61);
    sign_in(&s, &idp, "a", "eve@corp.test");
    assert_eq!(idp.jwks_fetches(), 3);
    // After an hour the cache is refreshed anyway.
    both(&s, &idp, 3601);
    sign_in(&s, &idp, "a", "eve@corp.test");
    assert_eq!(idp.jwks_fetches(), 4);
}

#[test]
fn domains_groups_and_sso_required() {
    let idp = MockIdp::start();
    let s = server("sso-policy", &idp, |c| {
        c.allowed_domains = vec!["corp.test".into()];
        c.required_groups = vec!["hitop-users".into()];
        c.sso_required = true;
    });
    let grouped = |sub: &str, email: &str, groups: Value| {
        let (flow, reply) = run(&s, &idp, None, |f| {
            let mut c = idp.claims(sub, email, &f.nonce);
            c["groups"] = groups;
            idp.sign(&c)
        });
        (flow, reply)
    };
    let (_, r) = grouped("f", "fay@other.test", json!(["hitop-users"]));
    assert_eq!(r.status, 400);
    assert_eq!(refused_reason(&s), "domain_not_allowed");
    let (_, r) = grouped("f", "fay@corp.test", json!(["staff"]));
    assert_eq!(r.status, 400);
    assert_eq!(refused_reason(&s), "group_not_allowed");
    let (_, r) = grouped("f", "fay@corp.test", json!(["/hitop-users"]));
    assert_eq!(r.status, 200, "Keycloak-style full group paths count too");
    // A hint from another domain is refused before the provider is involved.
    assert_eq!(s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[2; 32]), "email": "x@other.test" }))).status, 403);
    // SSO required: no sign-in mails or passkeys for corp.test; other domains keep them.
    let r = s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "fay@corp.test" })));
    assert_eq!((r.status, r.json["error"].as_str()), (403, Some("sso_required")));
    assert_eq!(s.call("POST", "/v1/auth/passkey/login/start", None, Some(json!({ "email": "fay@corp.test" }))).status, 403);
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "gus@other.test" }))).status, 202);
    assert_eq!(refused_reason(&s), "sso_required");
}

#[test]
fn a_link_mailed_before_sso_became_required_stops_working() {
    let idp = MockIdp::start();
    let before = Server::new("sso-late");
    assert_eq!(before.call("POST", "/v1/auth/email", None, Some(json!({ "email": "hal@corp.test" }))).status, 202);
    let token = before.mailed_token("hal@corp.test");
    // Same database, restarted with SSO required.
    let mut c = idp.config();
    c.sso_required = true;
    let db = hitop_sync_server::Db::open(&before.app.state().config.database).unwrap();
    let mut config = before.app.state().config.clone();
    config.oidc = Some(c);
    let after = hitop_sync_server::SyncApp::new(hitop_sync_server::State::new(db, hitop_sync_server::Mailer::Off, config).unwrap());
    let req = rust_web_server::request::Request {
        method: "POST".into(),
        request_uri: "/v1/auth/email/verify".into(),
        http_version: "HTTP/1.1".into(),
        headers: vec![],
        body: json!({ "token": token }).to_string().into_bytes().into(),
    };
    let conn = rust_web_server::server::ConnectionInfo::new(rust_web_server::server::Address { ip: "127.0.0.1".into(), port: 1 }, rust_web_server::server::Address { ip: "127.0.0.1".into(), port: 2 });
    use rust_web_server::application::Application;
    assert_eq!(after.execute(&req, &conn).unwrap().status_code, 403);
}

#[test]
fn config_errors_are_reported_and_audited() {
    let idp = MockIdp::start();
    let s = server("sso-config", &idp, |_| {});
    idp.inner.lock().unwrap().claimed_issuer = Some("https://impostor.example".into());
    let r = s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[3; 32]) })));
    assert_eq!((r.status, r.json["error"].as_str()), (503, Some("sso_unavailable")));
    let row = audit_rows(&s, "").pop().unwrap();
    assert_eq!(row["event"], "sso_config_error");
    assert!(row["detail"]["error"].as_str().unwrap().contains("impostor"), "{row}");
    // Nothing listening.
    let mut c = idp.config();
    c.issuer = "http://127.0.0.1:9/realm".into();
    let dead = Server::with("sso-dead", |config| config.oidc = Some(c));
    assert_eq!(dead.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[3; 32]) }))).status, 503);
    // Settings: all three or none; https unless local; assume-verified only with domains.
    let env = |pairs: &'static [(&'static str, &'static str)]| move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string());
    assert!(OidcConfig::from_env(&env(&[])).unwrap().is_none());
    assert!(OidcConfig::from_env(&env(&[("HITOP_SYNC_OIDC_ISSUER", "https://id.example")])).is_err());
    let full = [("HITOP_SYNC_OIDC_ISSUER", "https://id.example/"), ("HITOP_SYNC_OIDC_CLIENT_ID", "c"), ("HITOP_SYNC_OIDC_CLIENT_SECRET", "s")];
    let c = OidcConfig::from_env(&env(Box::leak(Box::new(full)))).unwrap().unwrap();
    assert_eq!(c.issuer, "https://id.example");
    assert!(!format!("{c:?}").contains("\"s\""), "Debug never prints the secret");
    assert!(OidcConfig::from_env(&env(&[("HITOP_SYNC_OIDC_ISSUER", "http://id.example"), ("HITOP_SYNC_OIDC_CLIENT_ID", "c"), ("HITOP_SYNC_OIDC_CLIENT_SECRET", "s")])).is_err());
    assert!(OidcConfig::from_env(&env(&[("HITOP_SYNC_OIDC_ISSUER", "https://id.example"), ("HITOP_SYNC_OIDC_CLIENT_ID", "c"), ("HITOP_SYNC_OIDC_CLIENT_SECRET", "s"), ("HITOP_SYNC_OIDC_ASSUME_VERIFIED", "1")])).is_err());
    assert!(OidcConfig::from_env(&env(&[
        ("HITOP_SYNC_OIDC_ISSUER", "https://id.example"),
        ("HITOP_SYNC_OIDC_CLIENT_ID", "c"),
        ("HITOP_SYNC_OIDC_CLIENT_SECRET", "s"),
        ("HITOP_SYNC_OIDC_ASSUME_VERIFIED", "1"),
        ("HITOP_SYNC_OIDC_ALLOWED_DOMAINS", "Corp.Test")
    ]))
    .unwrap()
    .is_some_and(|c| c.assume_verified && c.allowed_domains == ["corp.test"]));
}

// --- single sign-on per organization (HITOP Cloud C5, orgsso.rs) -------------------------------

use common::dns::MockDns;
use common::orgsso;

fn org_idp_config(idp: &MockIdp, name: &str) -> Value {
    json!({ "issuer": idp.issuer, "clientId": idp::CLIENT_ID, "clientSecret": idp::CLIENT_SECRET, "name": name })
}

/// A sign-in through an org's provider: start with `extra` (`email` or `org`), the provider answers
/// with `token(flow)`, callback. The session, or the refusal reason from the audit log.
fn org_sign_in(s: &Server, idp: &MockIdp, extra: Value, token: impl FnOnce(&SsoFlow) -> String) -> Result<String, String> {
    let mut extra = extra;
    extra["returnTo"] = json!("http://127.0.0.1:45678/done");
    let flow = orgsso::start(s, extra).unwrap_or_else(|r| panic!("start: {} {}", r.status, r.json));
    idp.next(token(&flow), &flow.pkce);
    let reply = sso_callback(s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state));
    match sso_code(&reply) {
        Some(code) => {
            let r = sso_finish(s, &code, &flow.verifier);
            assert_eq!(r.status, 200, "{}", r.json);
            Ok(r.json["session"].as_str().unwrap().to_string())
        }
        None => Err(refused_reason(s)),
    }
}

fn email_of(s: &Server, session: &str) -> String {
    s.call("GET", "/v1/me", Some(session), None).json["email"].as_str().unwrap().to_string()
}

#[test]
fn two_orgs_two_issuers_each_provider_signs_in_only_its_own_domains() {
    let dns = MockDns::start();
    let s = orgsso::server("sso-orgs", &dns, |_| {});
    let (idp_a, idp_b) = (MockIdp::start(), MockIdp::start());
    let ada = Device::new(&s, "ada@a.test");
    let ben = Device::new(&s, "ben@b.test");
    let a = orgsso::org(&s, &ada, "Org A");
    let b = orgsso::org(&s, &ben, "Org B");
    orgsso::claim(&s, &dns, &ada, &a, "a.test");
    orgsso::claim(&s, &dns, &ben, &b, "b.test");
    assert_eq!(orgsso::configure(&s, &ada, &a, org_idp_config(&idp_a, "A login")).status, 200);
    assert_eq!(orgsso::configure(&s, &ben, &b, org_idp_config(&idp_b, "B login")).status, 200);

    // The sign-in page learns the org from the address; the server has no SSO of its own.
    let info = s.call("GET", "/v1/auth/sso?email=cy@a.test", None, None).json;
    assert_eq!((info["enabled"].as_bool(), info["name"].as_str(), info["org"].as_str()), (Some(true), Some("A login"), Some(a.as_str())));
    let plain = s.call("GET", "/v1/auth/sso", None, None).json;
    assert_eq!((plain["enabled"].as_bool(), plain["orgSso"].as_bool()), (Some(true), Some(true)));
    // A 1.1/1.2 client with no address hint: told to give it.
    let r = orgsso::start(&s, json!({})).err().unwrap();
    assert_eq!(r.status, 400);
    assert!(r.json["message"].as_str().unwrap().contains("--email"), "{}", r.json);

    // Ada signs in through A's provider (by the address hint) into her account.
    let session = org_sign_in(&s, &idp_a, json!({ "email": "ada@a.test" }), |f| {
        assert!(f.url.as_str().starts_with(&idp_a.issuer), "A's provider");
        idp_a.sign(&idp_a.claims("a-ada", "ada@a.test", &f.nonce))
    })
    .unwrap();
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["id"], ada.user_id.as_str());
    // Ben through B's, by org id.
    let session = org_sign_in(&s, &idp_b, json!({ "org": b }), |f| idp_b.sign(&idp_b.claims("b-ben", "ben@b.test", &f.nonce))).unwrap();
    assert_eq!(email_of(&s, &session), "ben@b.test");

    // B's provider can't sign in a user of A, whatever it says about the address.
    let why = org_sign_in(&s, &idp_b, json!({ "org": b }), |f| idp_b.sign(&idp_b.claims("b-mallory", "ada@a.test", &f.nonce))).unwrap_err();
    assert_eq!(why, "domain_not_allowed");
    // Nor an address of no verified domain.
    assert_eq!(org_sign_in(&s, &idp_b, json!({ "org": b }), |f| idp_b.sign(&idp_b.claims("b-x", "x@elsewhere.test", &f.nonce))).unwrap_err(), "domain_not_allowed");
    // A's sign-in doesn't take a token from B's provider (other keys, other issuer).
    assert_eq!(org_sign_in(&s, &idp_a, json!({ "org": a }), |f| idp_b.sign(&idp_a.claims("a-ada", "ada@a.test", &f.nonce))).unwrap_err(), "bad_token");
    assert_eq!(org_sign_in(&s, &idp_a, json!({ "org": a }), |f| idp_a.sign(&idp_b.claims("a-ada", "ada@a.test", &f.nonce))).unwrap_err(), "bad_token");

    // One identity per (issuer, subject): each org's issuer.
    let ids = s.app.state().db.tx(|tx| tx.query("SELECT issuer, email FROM oidc_identities ORDER BY email", &[])).unwrap();
    let ids: Vec<(String, String)> = ids.iter().map(|r| (r.text(0), r.text(1))).collect();
    assert_eq!(ids, vec![(idp_a.issuer.clone(), "ada@a.test".into()), (idp_b.issuer.clone(), "ben@b.test".into())]);
    // The client secret is sealed at rest and never sent back.
    assert!(!s.disk_bytes().windows(idp::CLIENT_SECRET.len()).any(|w| w == idp::CLIENT_SECRET.as_bytes()), "the secret isn't on disk in the clear");
    let got = s.call("GET", &format!("/v1/orgs/{a}/sso"), Some(&ada.session), None).json;
    assert_eq!((got["config"]["hasSecret"].as_bool(), got["config"]["clientId"].as_str()), (Some(true), Some(idp::CLIENT_ID)));
    assert!(!got.to_string().contains(idp::CLIENT_SECRET));
}

#[test]
fn org_sso_required_owner_only_and_the_servers_own_sso_cant_get_around_it() {
    let dns = MockDns::start();
    let (idp_a, idp_env) = (MockIdp::start(), MockIdp::start());
    let env = idp_env.config();
    let s = orgsso::server("sso-org-required", &dns, |c| c.oidc = Some(env));
    let ada = Device::new(&s, "ada@a.test");
    let a = orgsso::org(&s, &ada, "Org A");
    // "SSO required" needs a verified domain; then only owners set it, with a session.
    let mut cfg = org_idp_config(&idp_a, "A login");
    cfg["required"] = json!(true);
    assert_eq!(orgsso::configure(&s, &ada, &a, cfg.clone()).status, 400);
    orgsso::claim(&s, &dns, &ada, &a, "a.test");
    let cy = Device::new(&s, "cy@a.test");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{a}/members"), Some(&ada.session), Some(json!({ "email": "cy@a.test", "role": "admin" }))).status, 201);
    assert_eq!(orgsso::configure(&s, &cy, &a, cfg.clone()).status, 403, "admins don't change SSO");
    let token = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read", "write"] }))).json["token"].as_str().unwrap().to_string();
    assert_eq!(s.call("PUT", &format!("/v1/orgs/{a}/sso"), Some(&token), Some(cfg.clone())).status, 403, "not with an access token");
    assert_eq!(orgsso::configure(&s, &ada, &a, cfg).status, 200);

    // The domain's addresses sign in only through the org's provider.
    let r = s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@a.test" })));
    assert_eq!((r.status, r.json["error"].as_str()), (403, Some("sso_required")));
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@other.test" }))).status, 202);
    // The server's own SSO doesn't either (its provider would sign in anyone).
    let (_, reply) = run(&s, &idp_env, None, |f| idp_env.sign(&idp_env.claims("env-ada", "ada@a.test", &f.nonce)));
    assert!(sso_code(&reply).is_none());
    assert_eq!(refused_reason(&s), "sso_required");
    // Through the org's provider it works.
    let session = org_sign_in(&s, &idp_a, json!({ "email": "ada@a.test" }), |f| idp_a.sign(&idp_a.claims("a-ada", "ada@a.test", &f.nonce))).unwrap();
    assert_eq!(email_of(&s, &session), "ada@a.test");
    // Turned off: mail sign-in comes back.
    assert_eq!(s.call("DELETE", &format!("/v1/orgs/{a}/sso"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "dee@a.test" }))).status, 202);
    let events: Vec<String> = audit_rows(&s, &format!("org:{a}")).iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    for e in ["org_domain_added", "org_domain_verified", "org_sso_updated", "org_sso_deleted"] {
        assert!(events.iter().any(|x| x == e), "{e} in {events:?}");
    }
    assert!(!audit_rows(&s, &format!("org:{a}")).iter().any(|r| r.to_string().contains(idp::CLIENT_SECRET)));
}

#[test]
fn org_sso_is_off_without_a_host_key_and_a_business_plan_on_hitop_cloud() {
    // The hosted test server: no host key, so nothing of it is reachable (owner decision Q15).
    let s = Server::new("sso-org-off");
    let ada = Device::new(&s, "ada@a.test");
    let a = orgsso::org(&s, &ada, "Org A");
    assert_eq!(s.call("GET", &format!("/v1/orgs/{a}/sso"), Some(&ada.session), None).json["available"], false);
    let r = s.call("PUT", &format!("/v1/orgs/{a}/sso"), Some(&ada.session), Some(json!({ "issuer": "https://idp.example", "clientId": "c", "clientSecret": "s" })));
    assert_eq!((r.status, r.json["error"].as_str()), (404, Some("org_sso_off")));
    assert_eq!(s.call("POST", &format!("/v1/orgs/{a}/domains"), Some(&ada.session), Some(json!({ "domain": "a.test" }))).status, 404);
    assert_eq!(s.call("GET", "/v1/auth/sso", None, None).json, json!({ "enabled": false }));
    assert_eq!(s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[0; 32]), "org": a }))).status, 404);

    // HITOP Cloud: Business or Enterprise.
    let dns = MockDns::start();
    let s = orgsso::server("sso-org-plan", &dns, |c| c.plan_limits = true);
    let ada = Device::new(&s, "ada@a.test");
    let a = orgsso::org(&s, &ada, "Org A");
    let r = s.call("POST", &format!("/v1/orgs/{a}/domains"), Some(&ada.session), Some(json!({ "domain": "a.test" })));
    assert_eq!((r.status, r.json["limit"].as_str()), (402, Some("sso")), "{}", r.json);
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{personal}/domains"), Some(&ada.session), Some(json!({ "domain": "a.test" }))).status, 400, "not a personal account");
    hitop_sync_server::limits::set_plan(s.app.state(), &a, "business", 5).unwrap();
    orgsso::claim(&s, &dns, &ada, &a, "a.test");
    let idp = MockIdp::start();
    assert_eq!(orgsso::configure(&s, &ada, &a, org_idp_config(&idp, "A")).status, 200);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{a}/sso/test"), Some(&ada.session), None).json, json!({ "ok": true }));
    // A lapsed plan locks nobody out: sign-in keeps working, changes don't.
    hitop_sync_server::limits::set_plan(s.app.state(), &a, "free", 0).unwrap();
    let session = org_sign_in(&s, &idp, json!({ "email": "ada@a.test" }), |f| idp.sign(&idp.claims("a-ada", "ada@a.test", &f.nonce))).unwrap();
    assert_eq!(email_of(&s, &session), "ada@a.test");
    assert_eq!(orgsso::configure(&s, &ada, &a, org_idp_config(&idp, "A2")).status, 402);
}
