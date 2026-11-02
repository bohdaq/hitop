//! HITOP Cloud C2 abuse limits through the whole app: request rates per client address and per
//! session (ratelimit.rs) and body size caps per kind of request.

mod common;

use common::{create_workspace, seal, Device, Server};
use hitop_sync_server::ratelimit::RateLimits;
use serde_json::json;

const WS: &str = "aa00000000000000000000000000000a";

fn sign_in_start(s: &Server, email: &str, xff: &str) -> common::Reply {
    s.raw("POST", "/v1/auth/email", None, json!({ "email": email }).to_string().into_bytes(), &[("X-Forwarded-For", xff)])
}

#[test]
fn sign_in_requests_are_limited_per_client_address() {
    let s = Server::with("abuse-auth", |c| c.rate_limits = RateLimits { auth_per_minute: 3, ..RateLimits::defaults() });
    for i in 0..3 {
        assert_eq!(sign_in_start(&s, &format!("p{i}@example.com"), "198.51.100.7").status, 202);
    }
    let r = sign_in_start(&s, "p9@example.com", "198.51.100.7");
    assert_eq!(r.status, 429, "{}", r.json);
    assert_eq!(r.json["error"], "rate_limited");
    assert_eq!(r.json["limit"], "auth");
    let retry: i64 = r.header("Retry-After").unwrap().parse().unwrap();
    assert!((1..=60).contains(&retry));
    // Another client behind the same proxy (the full address counts, not the /24 the log keeps).
    assert_eq!(sign_in_start(&s, "p9@example.com", "198.51.100.8").status, 202);
    // The window passes.
    s.advance(61);
    assert_eq!(sign_in_start(&s, "p9@example.com", "198.51.100.7").status, 202);
}

#[test]
fn writes_are_limited_per_session_reads_never() {
    let s = Server::with("abuse-write", |c| c.rate_limits = RateLimits { write_per_minute: 5, create_per_hour: 2, ..RateLimits::defaults() });
    let ada = Device::new(&s, "ada@example.com"); // 1 write (the device key, a create)
    let key = create_workspace(&s, &ada, WS); // 2 (create)
    let put = |v: u64| s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&ada.session), Some(json!({ "version": v, "vv": { "a": v }, "epoch": 1, "content": seal(&key, WS, "a", v, 1, b"x") })));
    assert_eq!(put(1).status, 200);
    assert_eq!(put(2).status, 200);
    assert_eq!(put(3).status, 200);
    let r = put(4);
    assert_eq!((r.status, r.json["limit"].as_str()), (429, Some("write")), "{}", r.json);
    for _ in 0..50 {
        assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 200);
    }
    // Someone else isn't held up by Ada's writes.
    let bob = Device::new(&s, "bob@example.com");
    assert_eq!(s.call("POST", "/v1/orgs", Some(&bob.session), Some(json!({ "name": "B" }))).status, 201);
    // Creates: an hourly budget per session (Bob: the device key, the org, now a third).
    let r = s.call("POST", "/v1/orgs", Some(&bob.session), Some(json!({ "name": "C" })));
    assert_eq!((r.status, r.json["limit"].as_str()), (429, Some("create")), "{}", r.json);
    s.advance(3600);
    assert_eq!(put(4).status, 200);
    assert_eq!(s.call("POST", "/v1/orgs", Some(&bob.session), Some(json!({ "name": "C" }))).status, 201);
}

#[test]
fn off_means_no_limit() {
    let s = Server::with("abuse-off", |c| c.rate_limits = RateLimits::off());
    for i in 0..80 {
        assert_eq!(sign_in_start(&s, &format!("p{i}@example.com"), "198.51.100.7").status, 202);
    }
}

#[test]
fn bodies_are_capped_by_kind() {
    let s = Server::new("abuse-body");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    // A JSON body over 1 MiB on anything but a blob upload: refused before it is parsed.
    let big = vec![b' '; (1 << 20) + 1];
    let r = s.raw("POST", "/v1/orgs", Some(&ada.session), big, &[]);
    assert_eq!((r.status, r.json["error"].as_str()), (413, Some("too_large")));
    // A blob of 3 MiB (about 4 MiB as base64 JSON) is still accepted.
    let content = seal(&key, WS, "big", 1, 1, &vec![7u8; 3 << 20]);
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/big"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": 1 }, "epoch": 1, "content": content })));
    assert_eq!(r.status, 200, "{}", r.json);
    // Beyond the blob limit: refused.
    let r = s.raw("PUT", &format!("/v1/workspaces/{WS}/blobs/big"), Some(&ada.session), vec![b' '; 6 << 20], &[]);
    assert_eq!(r.status, 413);
}
