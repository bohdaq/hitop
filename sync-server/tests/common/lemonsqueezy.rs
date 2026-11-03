#![allow(dead_code)]
//! A stand-in for the three Lemon Squeezy API calls the server makes (`POST /v1/checkouts`,
//! `PATCH /v1/subscription-items/:id`, `GET /v1/subscriptions/:id`), on 127.0.0.1, and the
//! webhook deliveries Lemon Squeezy would send (fixtures/lemonsqueezy/, signed like the real ones).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use hitop_sync_server::billing::{hmac_sha256, BillingConfig, Variant};
use serde_json::{json, Value};

use super::{Reply, Server};

pub const SECRET: &str = "ls-webhook-secret";
pub const API_KEY: &str = "ls-test-api-key";
pub const STORE: &str = "4242";
pub const PRO_ANNUAL: &str = "101";
pub const PRO_MONTHLY: &str = "102";
pub const TEAM_ANNUAL: &str = "201";
pub const TEAM_MONTHLY: &str = "202";

#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub authorized: bool,
    pub body: Value,
}

#[derive(Clone)]
pub struct MockLs {
    pub url: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl MockLs {
    pub fn start() -> MockLs {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let ls = MockLs { url, seen: Arc::default() };
        let me = ls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    headers.push(h.trim().to_string());
                }
                let len = headers.iter().find_map(|h| h.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let authorized = headers.iter().any(|h| h == &format!("authorization: Bearer {API_KEY}") || h == &format!("Authorization: Bearer {API_KEY}"));
                let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                me.seen.lock().unwrap().push(Seen { method: method.clone(), path: path.clone(), authorized, body: body.clone() });
                let (status, answer) = if !authorized { (401, json!({ "errors": [{ "detail": "Unauthenticated." }] })) } else { answer(&method, &path, &body) };
                let text = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/vnd.api+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        ls
    }

    pub fn config(&self) -> BillingConfig {
        BillingConfig {
            webhook_secret: SECRET.into(),
            store_id: STORE.into(),
            variants: vec![
                Variant { plan: "pro", period: "annual", id: PRO_ANNUAL.into() },
                Variant { plan: "pro", period: "monthly", id: PRO_MONTHLY.into() },
                Variant { plan: "team", period: "annual", id: TEAM_ANNUAL.into() },
                Variant { plan: "team", period: "monthly", id: TEAM_MONTHLY.into() },
            ],
            api_key: Some(API_KEY.into()),
            api_url: self.url.clone(),
            test_mode: true,
        }
    }

    pub fn last(&self, method: &str) -> Seen {
        self.seen.lock().unwrap().iter().rev().find(|s| s.method == method).cloned().expect("a request")
    }
}

fn answer(method: &str, path: &str, body: &Value) -> (u16, Value) {
    match (method, path) {
        ("POST", "/v1/checkouts") => {
            let ok = body["data"]["type"] == "checkouts" && body["data"]["relationships"]["store"]["data"]["id"] == STORE;
            if !ok {
                return (422, json!({ "errors": [{ "detail": "bad checkout" }] }));
            }
            (201, json!({ "data": { "type": "checkouts", "id": "c0ffee", "attributes": { "url": "https://hitop-test.lemonsqueezy.com/checkout/custom/c0ffee?signature=x" } } }))
        }
        ("PATCH", p) if p.starts_with("/v1/subscription-items/") => {
            let id = p.trim_start_matches("/v1/subscription-items/");
            let q = body["data"]["attributes"]["quantity"].as_i64().unwrap_or(1);
            (200, json!({ "data": { "type": "subscription-items", "id": id, "attributes": { "subscription_id": 1, "price_id": 1, "quantity": q, "is_usage_based": false, "created_at": "2026-11-03T10:00:00.000000Z", "updated_at": "2026-11-03T11:00:00.000000Z" } } }))
        }
        ("GET", p) if p.starts_with("/v1/subscriptions/") => {
            let id = p.trim_start_matches("/v1/subscriptions/");
            (200, json!({ "data": { "type": "subscriptions", "id": id, "attributes": { "urls": { "customer_portal": format!("https://hitop-test.lemonsqueezy.com/billing?sub={id}&signature=fresh") } } } }))
        }
        _ => (404, json!({ "errors": [{ "detail": "Not Found" }] })),
    }
}

/// A fixture body (fixtures/lemonsqueezy/<name>.json).
pub fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lemonsqueezy").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A subscription event: `sub` with `variant`, `status`, `quantity`, `updated` (ISO time).
pub fn subscription(event: &str, sub: &str, variant: &str, status: &str, quantity: i64, updated: &str, custom: Value) -> Value {
    let mut b = fixture("subscription");
    b["meta"]["event_name"] = json!(event);
    b["meta"]["custom_data"] = custom;
    b["data"]["id"] = json!(sub);
    let a = &mut b["data"]["attributes"];
    a["store_id"] = json!(STORE.parse::<u64>().unwrap());
    a["variant_id"] = json!(variant.parse::<u64>().unwrap());
    a["order_id"] = json!(format!("9{sub}").parse::<u64>().unwrap());
    a["status"] = json!(status);
    a["cancelled"] = json!(status == "cancelled");
    a["first_subscription_item"]["id"] = json!(format!("7{sub}").parse::<u64>().unwrap());
    a["first_subscription_item"]["quantity"] = json!(quantity);
    a["updated_at"] = json!(updated);
    if matches!(status, "cancelled" | "expired") {
        a["ends_at"] = json!("2027-11-03T10:00:00.000000Z");
    }
    b
}

/// A subscription invoice event for `sub`.
pub fn invoice(event: &str, sub: &str, refunded: bool, updated: &str) -> Value {
    let mut b = fixture("subscription_invoice");
    b["meta"]["event_name"] = json!(event);
    let a = &mut b["data"]["attributes"];
    a["store_id"] = json!(STORE.parse::<u64>().unwrap());
    a["subscription_id"] = json!(sub.parse::<u64>().unwrap());
    a["refunded"] = json!(refunded);
    a["status"] = json!(if refunded { "refunded" } else if event == "subscription_payment_failed" { "pending" } else { "paid" });
    a["updated_at"] = json!(updated);
    b
}

/// An order event for order `id`.
pub fn order(event: &str, id: &str, refunded: bool, updated: &str) -> Value {
    let mut b = fixture("order");
    b["meta"]["event_name"] = json!(event);
    b["data"]["id"] = json!(id);
    let a = &mut b["data"]["attributes"];
    a["store_id"] = json!(STORE.parse::<u64>().unwrap());
    a["refunded"] = json!(refunded);
    a["status"] = json!(if refunded { "refunded" } else { "paid" });
    a["updated_at"] = json!(updated);
    b
}

pub fn sign(body: &[u8]) -> String {
    hmac_sha256(SECRET.as_bytes(), body).iter().map(|b| format!("{b:02x}")).collect()
}

/// Posts `body` as Lemon Squeezy would (signed, with X-Event-Name).
pub fn deliver(s: &Server, body: &Value) -> Reply {
    let bytes = body.to_string().into_bytes();
    let sig = sign(&bytes);
    let event = body["meta"]["event_name"].as_str().unwrap_or("").to_string();
    s.raw("POST", "/v1/billing/lemonsqueezy", None, bytes, &[("Content-Type", "application/json"), ("X-Event-Name", &event), ("X-Signature", &sig)])
}

/// Delivers and expects 200 with `outcome`.
pub fn delivered(s: &Server, body: &Value, outcome: &str) -> Reply {
    let r = deliver(s, body);
    assert_eq!((r.status, r.json["outcome"].as_str()), (200, Some(outcome)), "{} {}", body["meta"]["event_name"], r.json);
    r
}
