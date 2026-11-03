#![allow(dead_code)]
//! A stand-in for the three Stripe API calls the server makes (`POST /v1/checkout/sessions`,
//! `POST /v1/subscriptions/:id`, `POST /v1/billing_portal/sessions`), on 127.0.0.1, form-encoded
//! like the real API, and the webhook deliveries Stripe would send (fixtures/stripe/, signed like
//! the real ones with the server's clock).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use hitop_sync_server::billing::{stripe_signature, BillingConfig, Price, STRIPE_VERSION};
use serde_json::{json, Value};

use super::{Reply, Server};

pub const WEBHOOK_SECRET: &str = "whsec_test_hitop";
pub const SECRET_KEY: &str = "sk_test_hitop";
pub const PRO_ANNUAL: &str = "price_pro_annual";
pub const PRO_MONTHLY: &str = "price_pro_monthly";
pub const TEAM_ANNUAL: &str = "price_team_annual";
pub const TEAM_MONTHLY: &str = "price_team_monthly";
pub const BUSINESS_ANNUAL: &str = "price_business_annual";
pub const BUSINESS_MONTHLY: &str = "price_business_monthly";
/// 2026-11-03 10:40 UTC: the `created` the tests count from.
pub const T0: i64 = 1_793_700_000;
/// A seat count the fake refuses with Stripe's 402 card error (a declined proration charge).
pub const DECLINED_SEATS: i64 = 99;

#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    /// The secret key as a bearer token and the pinned `Stripe-Version`.
    pub authorized: bool,
    pub form: Vec<(String, String)>,
}

impl Seen {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.form.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// The `metadata[...]` (or `subscription_data[metadata][...]`) entries under `prefix` as JSON.
    pub fn metadata(&self, prefix: &str) -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.form {
            if let Some(name) = k.strip_prefix(prefix).and_then(|r| r.strip_prefix('[')).and_then(|r| r.strip_suffix(']')) {
                m.insert(name.to_string(), json!(v));
            }
        }
        Value::Object(m)
    }
}

#[derive(Clone)]
pub struct MockStripe {
    pub url: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl MockStripe {
    pub fn start() -> MockStripe {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let mock = MockStripe { url, seen: Arc::default() };
        let me = mock.clone();
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
                let header = |name: &str| headers.iter().find_map(|h| h.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()));
                let len = header("content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let authorized = header("authorization").as_deref() == Some(&format!("Bearer {SECRET_KEY}")) && header("stripe-version").as_deref() == Some(STRIPE_VERSION);
                let (path, query) = path.split_once('?').map(|(p, q)| (p.to_string(), q.to_string())).unwrap_or((path, String::new()));
                let form: Vec<(String, String)> = url::form_urlencoded::parse(if method == "GET" { query.as_bytes() } else { &body }).into_owned().collect();
                let seen = Seen { method: method.clone(), path: path.clone(), authorized, form };
                me.seen.lock().unwrap().push(seen.clone());
                let (status, answer) = if !authorized { (401, json!({ "error": { "type": "invalid_request_error", "message": "Invalid API Key provided" } })) } else { answer(&seen) };
                let text = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        mock
    }

    /// Explicit price ids for Pro and Team (no Business), as `HITOP_SYNC_STRIPE_PRICE_*` would set.
    /// The fake's price list has no Business either, so Business isn't on sale.
    pub fn config(&self) -> BillingConfig {
        let prices = vec![
            Price { plan: "pro", period: "annual", id: PRO_ANNUAL.into() },
            Price { plan: "pro", period: "monthly", id: PRO_MONTHLY.into() },
            Price { plan: "team", period: "annual", id: TEAM_ANNUAL.into() },
            Price { plan: "team", period: "monthly", id: TEAM_MONTHLY.into() },
        ];
        BillingConfig::new(SECRET_KEY, WEBHOOK_SECRET, prices, &self.url, false)
    }

    /// Only the two secrets: every price found by its lookup key.
    pub fn config_by_lookup_key(&self) -> BillingConfig {
        BillingConfig::new(SECRET_KEY, WEBHOOK_SECRET, vec![], &self.url, false)
    }

    pub fn count(&self, method: &str, path: &str) -> usize {
        self.seen.lock().unwrap().iter().filter(|s| s.method == method && s.path == path).count()
    }

    pub fn last(&self, path_prefix: &str) -> Seen {
        self.seen.lock().unwrap().iter().rev().find(|s| s.path.starts_with(path_prefix)).cloned().expect("a request")
    }
}

fn error(status: u16, kind: &str, code: &str, message: &str) -> (u16, Value) {
    (status, json!({ "error": { "type": kind, "code": code, "message": message } }))
}

/// Parameters Managed Payments refuses on a Checkout Session (the owner's Dashboard list,
/// 2026-11-03), as form keys.
pub const REFUSED_BY_MANAGED_PAYMENTS: &[&str] = &[
    "automatic_tax",
    "tax_id_collection",
    "subscription_data[default_tax_rates]",
    "payment_method_collection",
    "payment_method_configuration",
    "payment_method_options",
    "payment_method_types",
    "saved_payment_method_options",
    "customer_update[name]",
    "customer_update[address]",
    "shipping_address_collection",
    "shipping_options",
    "subscription_data[application_fee_percent]",
    "subscription_data[on_behalf_of]",
    "subscription_data[transfer_data]",
    "subscription_data[invoice_settings]",
];

/// The form keys of `s` that Managed Payments refuses.
pub fn refused_keys(s: &Seen) -> Vec<String> {
    s.form.iter().map(|(k, _)| k.clone()).filter(|k| REFUSED_BY_MANAGED_PAYMENTS.iter().any(|r| k == r || k.starts_with(&format!("{r}[")))).collect()
}

/// The lookup keys the fake's price list has (the owner's six but Business, which tests add
/// through explicit ids), and the id it gives each.
pub const LISTED: &[&str] = &["pro_monthly", "pro_yearly", "team_monthly", "team_yearly"];
pub fn listed_id(key: &str) -> String {
    format!("price_lk_{key}")
}

fn answer(s: &Seen) -> (u16, Value) {
    match (s.method.as_str(), s.path.as_str()) {
        ("GET", "/v1/prices") => {
            let wanted: Vec<&str> = s.form.iter().filter(|(k, _)| k == "lookup_keys[]").map(|(_, v)| v.as_str()).collect();
            let data: Vec<Value> = LISTED
                .iter()
                .filter(|k| wanted.contains(k))
                .map(|k| json!({ "id": listed_id(k), "object": "price", "active": true, "currency": "usd", "lookup_key": k, "recurring": { "interval": if k.ends_with("yearly") { "year" } else { "month" }, "usage_type": "licensed" }, "tax_behavior": "exclusive", "type": "recurring" }))
                .collect();
            (200, json!({ "object": "list", "data": data, "has_more": false, "url": "/v1/prices" }))
        }
        ("POST", "/v1/checkout/sessions") => {
            let ok = s.get("mode") == Some("subscription")
                && s.get("managed_payments[enabled]") == Some("true")
                && s.get("line_items[0][price]").is_some_and(|p| p.starts_with("price_"))
                && s.get("line_items[0][quantity]").is_some()
                && s.get("success_url").is_some()
                // Managed Payments refuses these (Stripe decides tax and payment methods).
                && refused_keys(s).is_empty();
            if !ok {
                return error(400, "invalid_request_error", "parameter_invalid", "bad checkout session");
            }
            (200, json!({ "id": "cs_test_hitop", "object": "checkout.session", "mode": "subscription", "url": "https://checkout.stripe.com/c/pay/cs_test_hitop#fid" }))
        }
        ("POST", "/v1/billing_portal/sessions") => {
            let customer = s.get("customer").unwrap_or("");
            if !customer.starts_with("cus_") || s.get("return_url").is_none() {
                return error(400, "invalid_request_error", "parameter_missing", "customer and return_url");
            }
            (200, json!({ "id": "bps_test", "object": "billing_portal.session", "customer": customer, "url": format!("https://billing.stripe.com/p/session/test_{customer}") }))
        }
        ("POST", p) if p.starts_with("/v1/subscriptions/") => {
            let id = p.trim_start_matches("/v1/subscriptions/");
            let item = s.get("items[0][id]").unwrap_or("");
            let q: i64 = s.get("items[0][quantity]").and_then(|q| q.parse().ok()).unwrap_or(1);
            if q == DECLINED_SEATS {
                return error(402, "card_error", "card_declined", "Your card was declined.");
            }
            (200, json!({ "id": id, "object": "subscription", "status": "active", "items": { "object": "list", "data": [{ "id": item, "object": "subscription_item", "quantity": q }] } }))
        }
        _ => error(404, "invalid_request_error", "resource_missing", "Unrecognized request URL"),
    }
}

/// A fixture body (fixtures/stripe/<name>.json).
pub fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stripe").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

static NEXT: AtomicU64 = AtomicU64::new(1);

fn envelope(name: &str, event: &str, created: i64) -> Value {
    let mut b = fixture(name);
    b["id"] = json!(format!("evt_test_{:06}", NEXT.fetch_add(1, Ordering::Relaxed)));
    b["type"] = json!(event);
    b["created"] = json!(created);
    b
}

/// `checkout.session.completed` for `sub` and `customer`, with the session's `metadata`.
pub fn session(sub: &str, customer: &str, metadata: Value, created: i64) -> Value {
    let mut b = envelope("checkout_session", "checkout.session.completed", created);
    let o = &mut b["data"]["object"];
    o["subscription"] = json!(sub);
    o["customer"] = json!(customer);
    o["client_reference_id"] = metadata["org_id"].clone();
    o["metadata"] = metadata;
    b
}

/// A `customer.subscription.*` event: `sub` (customer `cus_<sub>`) on `price`, Stripe `status`,
/// `quantity`, event `created`, the subscription's `metadata`.
pub fn subscription(event: &str, sub: &str, price: &str, status: &str, quantity: i64, created: i64, metadata: Value) -> Value {
    let mut b = envelope("subscription", event, created);
    let o = &mut b["data"]["object"];
    o["id"] = json!(sub);
    o["customer"] = json!(customer_of(sub));
    o["status"] = json!(status);
    o["metadata"] = metadata;
    if status == "canceled" {
        o["ended_at"] = json!(created);
        o["canceled_at"] = json!(created);
    }
    let item = &mut o["items"]["data"][0];
    item["id"] = json!(format!("si_{sub}"));
    item["price"]["id"] = json!(price);
    item["price"]["lookup_key"] = lookup_key_of(price).map_or(Value::Null, |k| json!(k));
    item["quantity"] = json!(quantity);
    item["subscription"] = json!(sub);
    b
}

/// The lookup key the owner's prices carry (the explicit test ids and the fake's listed ones).
pub fn lookup_key_of(price: &str) -> Option<String> {
    if let Some(k) = price.strip_prefix("price_lk_") {
        return Some(k.to_string());
    }
    let k = price.strip_prefix("price_")?.replace("_annual", "_yearly");
    ["pro_monthly", "pro_yearly", "team_monthly", "team_yearly", "business_monthly", "business_yearly"].contains(&k.as_str()).then_some(k)
}

/// The customer the fake gives subscription `sub`.
pub fn customer_of(sub: &str) -> String {
    format!("cus_{}", sub.trim_start_matches("sub_"))
}

/// A subscription set to end at the period's end (`cancel_at_period_end`), period ending at `end`.
pub fn cancelling(sub: &str, price: &str, quantity: i64, created: i64, end: i64, metadata: Value) -> Value {
    let mut b = subscription("customer.subscription.updated", sub, price, "active", quantity, created, metadata);
    b["data"]["object"]["cancel_at_period_end"] = json!(true);
    b["data"]["object"]["items"]["data"][0]["current_period_end"] = json!(end);
    b
}

/// An `invoice.*` event for `sub`.
pub fn invoice(event: &str, sub: &str, created: i64) -> Value {
    let mut b = envelope("invoice", event, created);
    let o = &mut b["data"]["object"];
    o["customer"] = json!(customer_of(sub));
    o["parent"]["subscription_details"]["subscription"] = json!(sub);
    o["status"] = json!(if event == "invoice.payment_failed" { "open" } else { "paid" });
    b
}

/// `charge.refunded` of a charge of `customer`: in full or in part.
pub fn refund(customer: &str, full: bool, created: i64) -> Value {
    let mut b = envelope("charge", "charge.refunded", created);
    let o = &mut b["data"]["object"];
    o["customer"] = json!(customer);
    o["refunded"] = json!(full);
    o["amount_refunded"] = json!(if full { 800 } else { 300 });
    b
}

/// The `Stripe-Signature` header for `bytes` at the server's now.
pub fn signature(s: &Server, bytes: &[u8]) -> String {
    let t = s.app.state().now();
    format!("t={t},v1={}", stripe_signature(WEBHOOK_SECRET, t, bytes))
}

/// Posts `body` as Stripe would (signed with the server's clock).
pub fn deliver(s: &Server, body: &Value) -> Reply {
    let bytes = body.to_string().into_bytes();
    let sig = signature(s, &bytes);
    s.raw("POST", "/v1/billing/stripe", None, bytes, &[("Content-Type", "application/json; charset=utf-8"), ("Stripe-Signature", &sig)])
}

/// Delivers and expects 200 with `outcome`.
pub fn delivered(s: &Server, body: &Value, outcome: &str) -> Reply {
    let r = deliver(s, body);
    assert_eq!((r.status, r.json["outcome"].as_str()), (200, Some(outcome)), "{} {}", body["type"], r.json);
    r
}
