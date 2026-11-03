#![allow(dead_code)]
//! Single sign-on per organization (orgsso.rs) in tests: a server with a host key and the test
//! resolver, an org with a verified domain and its provider configured, and a sign-in started for
//! an org.

use hitop_sync_server::hostkey::HostKey;
use hitop_sync_server::Config;
use serde_json::{json, Value};

use super::dns::MockDns;
use super::*;

pub fn host_key() -> HostKey {
    HostKey::from_bytes([7u8; 32])
}

/// A server with a host key and `dns` as its resolver.
pub fn server(name: &str, dns: &MockDns, tweak: impl FnOnce(&mut Config)) -> Server {
    let url = dns.url.clone();
    Server::with(name, |c| {
        c.host_key = Some(host_key());
        c.dns_url = Some(url);
        tweak(c);
    })
}

pub fn org(s: &Server, owner: &Device, name: &str) -> String {
    let r = s.call(
        "POST",
        "/v1/orgs",
        Some(&owner.session),
        Some(json!({ "name": name })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["id"].as_str().unwrap().to_string()
}

/// Claims `domain` for `org`, publishes the TXT record and verifies it.
pub fn claim(s: &Server, dns: &MockDns, owner: &Device, org: &str, domain: &str) {
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{org}/domains"),
        Some(&owner.session),
        Some(json!({ "domain": domain })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
    dns.set(
        r.json["record"].as_str().unwrap(),
        &[r.json["value"].as_str().unwrap()],
    );
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{org}/domains/{domain}/verify"),
        Some(&owner.session),
        None,
    );
    assert_eq!(
        (r.status, r.json["verified"].as_bool()),
        (200, Some(true)),
        "{}",
        r.json
    );
}

/// `PUT /v1/orgs/:org/sso`.
pub fn configure(s: &Server, owner: &Device, org: &str, config: Value) -> Reply {
    s.call(
        "PUT",
        &format!("/v1/orgs/{org}/sso"),
        Some(&owner.session),
        Some(config),
    )
}

/// A sign-in started with `extra` fields (`email`, `org`, `returnTo`) → the flow, or the error.
pub fn start(s: &Server, extra: Value) -> Result<SsoFlow, Reply> {
    use sha2::Digest;
    let verifier = b64(&[
        uuid::Uuid::new_v4().into_bytes(),
        uuid::Uuid::new_v4().into_bytes(),
    ]
    .concat());
    let challenge = b64(&sha2::Sha256::digest(verifier.as_bytes()));
    let mut body = json!({ "challenge": challenge });
    for (k, v) in extra.as_object().unwrap() {
        body[k] = v.clone();
    }
    let r = s.call("POST", "/v1/auth/sso/start", None, Some(body));
    if r.status != 200 {
        return Err(r);
    }
    let url = url::Url::parse(r.json["url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(
        q["redirect_uri"],
        format!("{ORIGIN}/v1/auth/sso/callback"),
        "one callback URL for every org"
    );
    Ok(SsoFlow {
        verifier,
        state: q["state"].clone(),
        nonce: q["nonce"].clone(),
        pkce: q["code_challenge"].clone(),
        url,
    })
}
