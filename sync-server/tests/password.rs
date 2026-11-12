//! Passwords (password.rs, THREAT_MODEL.md "Passwords"): sign-up with a mailed code, sign-in,
//! answers that don't tell whether an address has an account, the backoff after wrong passwords,
//! change and reset (which loses the password's key bundle), and what the database keeps. The
//! client side is hitop-sync-crypto's `derive_password_keys`, as every hitop client runs it.

mod common;

use common::pw::*;
use common::*;
use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

fn last_mail(s: &Server, email: &str) -> String {
    let outbox = s.outbox.lock().unwrap();
    outbox.iter().rev().find(|m| m.to == email).map(|m| format!("{}\n{}", m.subject, m.body)).unwrap_or_default()
}

#[test]
fn methods_say_what_a_sign_in_page_offers() {
    let s = Server::new("pw-methods");
    let m = s.call("GET", "/v1/auth/methods", None, None);
    assert_eq!(m.json, json!({ "password": { "enabled": true, "signup": true, "reset": true }, "emailCode": true, "passkey": true, "sso": null, "google": false, "github": false, "apple": false }));
    let off = Server::with("pw-off", |c| c.password.enabled = false);
    assert_eq!(off.call("GET", "/v1/auth/methods", None, None).json["password"]["enabled"], false);
    assert_eq!(off.call("POST", "/v1/auth/password/login", None, Some(json!({ "email": "a@x.test", "authKey": b64(&[0; 32]) }))).status, 404);
}

#[test]
fn sign_up_confirm_sign_in_and_nothing_secret_on_disk() {
    let s = Server::new("pw-signup");
    let (kdf, keys) = new_password("correct horse battery staple");
    let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": "Ada@Example.test", "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.status, 202);
    assert!(last_mail(&s, "ada@example.test").starts_with("Confirm your hitop sync account"));
    // Not signed up until the code is used: no account, no password.
    assert_eq!(login(&s, "ada@example.test", "correct horse battery staple").status, 401);
    let v = s.call("POST", "/v1/auth/password/verify", None, Some(json!({ "token": format!("https://x/signin#{}", s.mailed_token("ada@example.test")) })));
    assert_eq!(v.status, 200, "{}", v.json);
    assert_eq!(v.json["created"], true);
    let me = s.call("GET", "/v1/me", Some(v.json["session"].as_str().unwrap()), None);
    assert_eq!(me.json["password"]["set"], true);
    assert_eq!(me.json["bundles"], json!({}));
    assert_eq!(me.json["recovery"], Value::Null, "the 1.2 field still means the phrase only");
    // The code works once.
    assert_eq!(s.call("POST", "/v1/auth/password/verify", None, Some(json!({ "token": s.mailed_token("ada@example.test") }))).status, 401);

    let ok = login(&s, "ada@example.test", "correct horse battery staple");
    assert_eq!(ok.status, 200, "{}", ok.json);
    assert_eq!(ok.json["user"]["email"], "ada@example.test");
    // The params a client gets back are the ones it signed up with.
    let p = s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": "ada@example.test" })));
    assert_eq!(kdf_of(&p.json["kdf"]), kdf);

    // The database has a hash of the auth key, never the auth key, the bundle key or the password.
    let disk = s.disk_bytes();
    for secret in [keys.auth_key.to_vec(), b64(&keys.auth_key).into_bytes(), keys.bundle_key.to_vec(), b64(&keys.bundle_key).into_bytes(), b"correct horse battery staple".to_vec()] {
        assert!(!disk.windows(secret.len()).any(|w| w == secret.as_slice()), "a secret on disk");
    }
    let hash = s.app.state().db.tx(|tx| tx.query_one("SELECT hash, peppered FROM password_credentials", &[])).unwrap().unwrap();
    assert!(hash.text(0).starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{}", hash.text(0));
    assert_eq!(hash.int(1), 0);
    let events: Vec<String> = audit_rows(&s, "").iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert!(events.contains(&"password_set".to_string()) && events.contains(&"signin".to_string()));
}

#[test]
fn answers_never_tell_whether_an_address_has_an_account() {
    let s = Server::new("pw-enum");
    sign_up(&s, "has@x.test", "a long password 1");
    s.sign_in("nopass@x.test"); // an account by mail code, no password

    // Params: the same shape for all three, stable for the made-up ones.
    let params = |e: &str| s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": e }))).json["kdf"].clone();
    for e in ["has@x.test", "nopass@x.test", "nobody@x.test"] {
        let k = params(e);
        assert_eq!(k.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["alg", "m", "p", "salt", "t"], "{e}");
        assert!(kdf_of(&k).check().is_ok(), "{e}");
    }
    assert_eq!(params("nobody@x.test"), params("nobody@x.test"));
    assert_ne!(params("nobody@x.test"), params("other@x.test"));

    // Sign-in: one answer for a wrong address, an account without a password and a wrong password.
    let answers: Vec<(i16, Value)> = ["nobody@x.test", "nopass@x.test", "has@x.test"].iter().map(|e| {
        let r = login(&s, e, "wrong password here");
        (r.status, r.json)
    }).collect();
    for a in &answers {
        assert_eq!(a, &answers[0]);
    }
    assert_eq!(answers[0].0, 401);
    assert_eq!(answers[0].1["error"], "invalid_credentials");

    // Sign-up and forgot: 202 every time; the mails differ, the answers don't.
    let (kdf, keys) = new_password("another long one");
    for e in ["has@x.test", "nopass@x.test", "new@x.test"] {
        let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": e, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
        assert_eq!((r.status, r.json), (202, json!({ "sent": true })), "{e}");
        let f = s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": e })));
        assert_eq!((f.status, f.json), (202, json!({ "sent": true })), "{e}");
    }
    let outbox = s.outbox.lock().unwrap();
    let signup_mail = outbox.iter().rev().find(|m| m.to == "has@x.test" && m.subject.starts_with("Confirm")).unwrap();
    assert!(signup_mail.body.contains("already has one") && !signup_mail.body.contains("/signin#"));
    let nobody = outbox.iter().find(|m| m.to == "new@x.test" && m.subject.starts_with("Reset")).unwrap();
    assert!(nobody.body.contains("no account") && !nobody.body.contains("/signin#"));
}

#[test]
fn wrong_passwords_back_off_for_every_address_alike() {
    let s = Server::new("pw-backoff");
    sign_up(&s, "ada@x.test", "the right password");
    for e in ["ada@x.test", "ghost@x.test"] {
        for _ in 0..5 {
            assert_eq!(login(&s, e, "not it at all").status, 401, "{e}");
        }
        let r = login(&s, e, "the right password");
        assert_eq!(r.status, 429, "{e}: waits even with the right password");
        assert!((25..=30).contains(&r.json["retryAfter"].as_i64().unwrap()), "{}", r.json);
    }
    s.advance(31);
    assert_eq!(login(&s, "ada@x.test", "the right password").status, 200);
    // A success clears the count.
    assert_eq!(login(&s, "ada@x.test", "nope nope nope").status, 401);
    assert_eq!(login(&s, "ada@x.test", "the right password").status, 200);
    // Off with the rate limits.
    let off = Server::with("pw-backoff-off", |c| c.password.backoff = false);
    for _ in 0..8 {
        assert_eq!(login(&off, "ghost@x.test", "x x x x x x").status, 401);
    }
}

#[test]
fn an_existing_account_adds_a_password_by_sign_up_or_while_signed_in() {
    let s = Server::new("pw-add");
    let old = s.sign_in("old@x.test");
    let id = s.call("GET", "/v1/me", Some(&old), None).json["id"].clone();
    // Sign-up for an address with an account but no password: the code adds the password.
    let (session, _) = sign_up(&s, "old@x.test", "first password here");
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["id"], id);
    assert_eq!(login(&s, "old@x.test", "first password here").status, 200);

    // Signed in, no password yet: set one (no current password needed); mailed about it.
    let other = s.sign_in("other@x.test");
    let (kdf, keys) = new_password("other password 1");
    let r = s.call("POST", "/v1/me/password", Some(&other), Some(json!({ "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["bundle"], "none");
    assert!(last_mail(&s, "other@x.test").starts_with("Your hitop sync password was set"));
    assert_eq!(login(&s, "other@x.test", "other password 1").status, 200);
    // An access token can't.
    let pat = s.call("POST", "/v1/me/tokens", Some(&other), Some(json!({ "name": "t", "scopes": ["read", "write"] })));
    assert_eq!(pat.status, 201, "{}", pat.json);
    let r = s.call("POST", "/v1/me/password", Some(pat.json["token"].as_str().unwrap()), Some(json!({ "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.status, 403);
}

#[test]
fn weak_parameters_and_bad_keys_are_refused() {
    let s = Server::new("pw-weak");
    let (kdf, keys) = new_password("a fine password");
    for (field, value) in [("m", json!(1024)), ("t", json!(1)), ("p", json!(0)), ("alg", json!("argon2i")), ("salt", json!(b64(&[1; 8])))] {
        let mut k = kdf_json(&kdf);
        k[field] = value;
        let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": "w@x.test", "kdf": k, "authKey": b64(&keys.auth_key) })));
        assert_eq!(r.status, 400, "{field}");
    }
    let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": "w@x.test", "kdf": kdf_json(&kdf), "authKey": b64(&[1; 16]) })));
    assert_eq!(r.status, 400);
}

#[test]
fn change_password_reseals_the_bundle_and_signs_other_devices_out() {
    let s = Server::new("pw-change");
    let (laptop, bundle_key) = sign_up(&s, "ada@x.test", "old password 123");
    let desktop = login(&s, "ada@x.test", "old password 123").json["session"].as_str().unwrap().to_string();
    let me = s.call("GET", "/v1/me", Some(&laptop), None).json;
    let account = me["id"].as_str().unwrap().to_string();
    let bundle = crypto::seal_key_bundle(&bundle_key, &account, br#"{"v":1,"keyId":"k1"}"#).unwrap();
    let r = s.call("PUT", "/v1/me/recovery?method=password", Some(&laptop), Some(json!({ "blob": b64(&bundle) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(s.call("GET", "/v1/me", Some(&laptop), None).json["bundles"]["password"]["updatedAt"].is_number());

    // A new device: password sign-in, then the bundle opens with the key the same password gives.
    let fresh = login(&s, "ada@x.test", "old password 123");
    let p = s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": "ada@x.test" })));
    let k = crypto::derive_password_keys("old password 123", &kdf_of(&p.json["kdf"])).unwrap();
    let got = s.call("GET", "/v1/me/recovery?method=password", Some(fresh.json["session"].as_str().unwrap()), None);
    assert_eq!(crypto::open_key_bundle(&k.bundle_key, &account, &unb64(got.json["blob"].as_str().unwrap())).unwrap(), br#"{"v":1,"keyId":"k1"}"#);

    let (kdf, keys) = new_password("new password 456");
    let new_bundle = crypto::seal_key_bundle(&keys.bundle_key, &account, br#"{"v":1,"keyId":"k1"}"#).unwrap();
    let change = |current: Option<&[u8; 32]>, bundle: Option<&Vec<u8>>| {
        let mut b = json!({ "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) });
        if let Some(c) = current {
            b["currentAuthKey"] = json!(b64(c));
        }
        if let Some(x) = bundle {
            b["bundle"] = json!(b64(x));
        }
        s.call("POST", "/v1/me/password", Some(&laptop), Some(b))
    };
    assert_eq!(change(None, Some(&new_bundle)).json["error"], "current_password_required");
    assert_eq!(change(Some(&[9; 32]), Some(&new_bundle)).json["error"], "wrong_password");
    let r = change(Some(&k.auth_key), Some(&new_bundle));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["bundle"], "replaced");
    assert_eq!(r.json["sessionsEnded"], 2, "the desktop and the fresh device");
    assert_eq!(s.call("GET", "/v1/me", Some(&desktop), None).status, 401);
    assert_eq!(s.call("GET", "/v1/me", Some(&laptop), None).status, 200, "this session stays");
    assert_eq!(login(&s, "ada@x.test", "old password 123").status, 401);
    let new_session = login(&s, "ada@x.test", "new password 456");
    assert_eq!(new_session.status, 200);
    let got = s.call("GET", "/v1/me/recovery?method=password", Some(new_session.json["session"].as_str().unwrap()), None);
    assert_eq!(unb64(got.json["blob"].as_str().unwrap()), new_bundle);

    // A change without a bundle deletes the old one (it no longer opens).
    let (kdf2, keys2) = new_password("third password 789");
    let r = s.call("POST", "/v1/me/password", Some(&laptop), Some(json!({ "currentAuthKey": b64(&keys.auth_key), "kdf": kdf_json(&kdf2), "authKey": b64(&keys2.auth_key) })));
    assert_eq!(r.json["bundle"], "deleted");
    assert_eq!(s.call("GET", "/v1/me/recovery?method=password", Some(&laptop), None).status, 404);
    let events: Vec<String> = audit_rows(&s, "").iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert!(events.contains(&"password_changed".to_string()) && events.contains(&"password_change_failed".to_string()));
}

#[test]
fn reset_by_mail_signs_in_but_loses_the_password_bundle() {
    let s = Server::new("pw-reset");
    let (session, bundle_key) = sign_up(&s, "ada@x.test", "forgotten password");
    let account = s.call("GET", "/v1/me", Some(&session), None).json["id"].as_str().unwrap().to_string();
    let bundle = crypto::seal_key_bundle(&bundle_key, &account, b"bundle").unwrap();
    assert_eq!(s.call("PUT", "/v1/me/recovery?method=password", Some(&session), Some(json!({ "blob": b64(&bundle) }))).status, 200);
    let phrase = crypto::RecoveryPhrase::generate();
    let sealed = crypto::seal_recovery(&phrase, &account, b"bundle").unwrap();
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&session), Some(json!({ "blob": b64(&sealed) }))).status, 200);

    assert_eq!(s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "ada@x.test" }))).status, 202);
    let mail = last_mail(&s, "ada@x.test");
    assert!(mail.starts_with("Reset your hitop sync password") && mail.contains("recovery phrase"), "{mail}");
    let (kdf, keys) = new_password("brand new password");
    let token = s.mailed_token("ada@x.test");
    // The reset code isn't a sign-up code (nor the other way round).
    assert_eq!(s.call("POST", "/v1/auth/password/verify", None, Some(json!({ "token": token }))).status, 401);
    let r = s.call("POST", "/v1/auth/password/reset", None, Some(json!({ "token": token, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["bundleLost"], true);
    let fresh = r.json["session"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).status, 401, "every older session ends");
    assert_eq!(s.call("GET", "/v1/me/recovery?method=password", Some(&fresh), None).status, 404);
    assert_eq!(s.call("GET", "/v1/me/recovery?method=phrase", Some(&fresh), None).status, 200, "the phrase's bundle stays");
    assert_eq!(login(&s, "ada@x.test", "forgotten password").status, 401);
    assert_eq!(login(&s, "ada@x.test", "brand new password").status, 200);
    // Used once.
    let again = s.call("POST", "/v1/auth/password/reset", None, Some(json!({ "token": token, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(again.status, 401);
    // A second reset with no bundle left: nothing lost.
    s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "ada@x.test" })));
    let r = s.call("POST", "/v1/auth/password/reset", None, Some(json!({ "token": s.mailed_token("ada@x.test"), "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.json["bundleLost"], false);
}

#[test]
fn a_pepper_is_the_argon2_secret_and_old_hashes_still_verify() {
    let dir_name = "pw-pepper";
    let s = Server::with(dir_name, |c| c.password.pepper = Some(b"0123456789abcdef-pepper".to_vec()));
    sign_up(&s, "p@x.test", "peppered password");
    let row = s.app.state().db.tx(|tx| tx.query_one("SELECT peppered FROM password_credentials", &[])).unwrap().unwrap();
    assert_eq!(row.int(0), 1);
    assert_eq!(login(&s, "p@x.test", "peppered password").status, 200);
    assert_eq!(login(&s, "p@x.test", "wrong password!!").status, 401);
}

#[test]
fn sso_only_addresses_get_no_password() {
    let idp = common::idp::MockIdp::start();
    let mut c = idp.config();
    c.allowed_domains = vec!["corp.test".into()];
    c.sso_required = true;
    let s = Server::with("pw-sso", |config| config.oidc = Some(c));
    let (kdf, keys) = new_password("whatever password");
    let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": "a@corp.test", "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.json["error"], "sso_required");
    assert_eq!(s.call("POST", "/v1/auth/password/login", None, Some(json!({ "email": "a@corp.test", "authKey": b64(&keys.auth_key) }))).json["error"], "sso_required");
    assert_eq!(s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "a@corp.test" }))).json["error"], "sso_required");
}

#[test]
fn mails_per_address_are_limited() {
    let s = Server::new("pw-mails");
    for _ in 0..5 {
        assert_eq!(s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "m@x.test" }))).status, 202);
    }
    assert_eq!(s.call("POST", "/v1/auth/password/forgot", None, Some(json!({ "email": "m@x.test" }))).status, 429);
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": "other@x.test" }))).status, 202, "other addresses");
}
