//! Billing (HITOP Cloud C3, billing.rs): Lemon Squeezy webhook deliveries replayed from the
//! documented body shapes (fixtures/lemonsqueezy/) against a server with plan limits on and a fake
//! Lemon Squeezy API (common/lemonsqueezy.rs). Every event the spec lists; a bad signature refused
//! before the body is read; a replay changing nothing; out-of-order events; Team bought with 3
//! seats, a 4th member after buying a seat; payment failed → banner, expired → Free with every
//! byte still readable; refunds ending the plan; the trial; checkouts that sign the org.

mod common;

use common::lemonsqueezy::{self as ls, delivered, deliver, MockLs};
use common::*;
use serde_json::{json, Value};

fn server(name: &str, ls: &MockLs) -> Server {
    let c = ls.config();
    Server::with(name, |cfg| {
        cfg.plan_limits = true;
        cfg.billing = Some(c);
    })
}

fn org_create(s: &Server, d: &Device, name: &str) -> String {
    let r = s.call("POST", "/v1/orgs", Some(&d.session), Some(json!({ "name": name })));
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["id"].as_str().unwrap().to_string()
}

fn add_org_member(s: &Server, by: &Device, org: &str, email: &str) -> Reply {
    s.call("POST", &format!("/v1/orgs/{org}/members"), Some(&by.session), Some(json!({ "email": email, "role": "member" })))
}

/// Members and pending invites of `org`.
fn people(s: &Server, d: &Device, org: &str) -> usize {
    let r = s.call("GET", &format!("/v1/orgs/{org}/members"), Some(&d.session), None);
    r.json["members"].as_array().unwrap().len() + r.json["invites"].as_array().unwrap().len()
}

fn billing(s: &Server, d: &Device, org: &str) -> Value {
    let r = s.call("GET", &format!("/v1/orgs/{org}/billing"), Some(&d.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json
}

fn limits(s: &Server, d: &Device, org: &str) -> Value {
    let r = s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&d.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json
}

/// A checkout through the fake API; returns the `custom_data` it signed (what Lemon Squeezy then
/// sends back in every event of that purchase).
fn checkout(s: &Server, ls: &MockLs, d: &Device, org: &str, plan: &str, period: &str, seats: Option<i64>) -> Value {
    let mut body = json!({ "plan": plan, "period": period });
    if let Some(n) = seats {
        body["seats"] = json!(n);
    }
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&d.session), Some(body));
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(r.json["url"].as_str().unwrap().starts_with("https://hitop-test.lemonsqueezy.com/checkout/"));
    let seen = ls.last("POST");
    assert!(seen.authorized && seen.path == "/v1/checkouts");
    seen.body["data"]["attributes"]["checkout_data"]["custom"].clone()
}

fn events(s: &Server) -> Vec<(String, String)> {
    s.app.state().db.tx(|tx| tx.query("SELECT event_name, outcome FROM billing_events ORDER BY received_at, event_name", &[])).unwrap().iter().map(|r| (r.text(0), r.text(1))).collect()
}

#[test]
fn off_unless_configured() {
    let s = Server::with("billing-off", |c| c.plan_limits = true);
    let d = Device::new(&s, "ada@example.com");
    let body = ls::subscription("subscription_created", "1", ls::TEAM_ANNUAL, "active", 3, "2026-11-03T10:00:00.000000Z", json!({}));
    assert_eq!(deliver(&s, &body).status, 404);
    let org = org_create(&s, &d, "Acme");
    let b = billing(&s, &d, &org);
    assert_eq!((b["enabled"].as_bool(), b["checkout"].as_bool(), b["offers"].as_array().unwrap().len()), (Some(false), Some(false), 0));
    assert_eq!(b["trial"]["available"], false, "trials open with billing");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/trial"), Some(&d.session), Some(json!({}))).status, 409);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&d.session), Some(json!({ "plan": "team" }))).status, 404);
}

#[test]
fn a_bad_signature_is_refused_before_the_body_is_read() {
    let mock = MockLs::start();
    let s = server("billing-sig", &mock);
    let body = ls::subscription("subscription_created", "1", ls::TEAM_ANNUAL, "active", 3, "2026-11-03T10:00:00.000000Z", json!({}));
    let bytes = body.to_string().into_bytes();
    for sig in [None, Some("00".repeat(32)), Some(ls::sign(b"another body")), Some(ls::sign(&bytes)[..60].to_string())] {
        let mut headers = vec![("X-Event-Name", "subscription_created")];
        if let Some(s) = &sig {
            headers.push(("X-Signature", s.as_str()));
        }
        let r = s.raw("POST", "/v1/billing/lemonsqueezy", None, bytes.clone(), &headers);
        assert_eq!((r.status, r.json["error"].as_str()), (401, Some("bad_signature")), "{sig:?}");
    }
    // Not JSON, unsigned: still 401, not a parse error (nothing is parsed first).
    let r = s.raw("POST", "/v1/billing/lemonsqueezy", None, b"not json".to_vec(), &[("X-Signature", "x")]);
    assert_eq!(r.status, 401);
    // Signed garbage is a 400; a header naming another event too.
    let r = s.raw("POST", "/v1/billing/lemonsqueezy", None, b"not json".to_vec(), &[("X-Signature", &ls::sign(b"not json"))]);
    assert_eq!(r.status, 400);
    let r = s.raw("POST", "/v1/billing/lemonsqueezy", None, bytes.clone(), &[("X-Signature", &ls::sign(&bytes)), ("X-Event-Name", "order_created")]);
    assert_eq!(r.status, 400);
    assert!(events(&s).is_empty(), "nothing recorded");
    // Signed with the secret: accepted (unmatched: it names no org).
    delivered(&s, &body, "unmatched");
}

#[test]
fn team_with_three_seats_a_fourth_seat_then_payment_failed_and_expired() {
    let mock = MockLs::start();
    let s = server("billing-team", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    for e in ["bob@example.com", "cy@example.com"] {
        assert_eq!(add_org_member(&s, &ada, &org, e).status, 201);
    }
    let b = billing(&s, &ada, &org);
    assert_eq!((b["enabled"].as_bool(), b["checkout"].as_bool(), b["seatsUsed"].as_i64()), (Some(true), Some(true), Some(3)));
    assert_eq!(b["offers"], json!([{ "plan": "team", "period": "annual" }, { "plan": "team", "period": "monthly" }]));
    // Free: a 4th person is refused.
    assert_eq!(add_org_member(&s, &ada, &org, "dee@example.com").status, 402);
    // Pro isn't for a team org; fewer seats than used are refused.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "pro" }))).status, 400);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team", "seats": 2 }))).status, 400);

    let custom = checkout(&s, &mock, &ada, &org, "team", "annual", Some(3));
    let sent = mock.last("POST").body;
    let attrs = &sent["data"]["attributes"];
    assert_eq!(attrs["checkout_data"]["variant_quantities"], json!([{ "variant_id": 201, "quantity": 3 }]));
    assert_eq!(attrs["checkout_data"]["email"], "ada@example.com");
    assert_eq!(sent["data"]["relationships"]["store"]["data"]["id"], ls::STORE);
    assert_eq!((custom["org_id"].as_str(), custom["user_id"].as_str()), (Some(org.as_str()), Some(ada.user_id.as_str())));
    assert_eq!(custom["sig"].as_str().unwrap().len(), 64);

    // The purchase: order_created (recorded only), subscription_created, the first payment.
    let mut o = ls::order("order_created", "91", false, "2026-11-03T10:00:00.000000Z");
    o["meta"]["custom_data"] = custom.clone();
    delivered(&s, &o, "ignored");
    let created = ls::subscription("subscription_created", "1", ls::TEAM_ANNUAL, "active", 3, "2026-11-03T10:00:00.000000Z", custom.clone());
    delivered(&s, &created, "applied");
    delivered(&s, &ls::invoice("subscription_payment_success", "1", false, "2026-11-03T10:00:00.000000Z"), "ignored");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["plan"].as_str(), l["planInForce"].as_str(), l["seats"].as_i64(), l["limits"]["seats"].as_i64(), l["limits"]["guestViewers"].as_i64()), (Some("team"), Some("team"), Some(3), Some(3), Some(5)));
    // A replay of the same delivery changes nothing.
    delivered(&s, &created, "duplicate");
    // Already subscribed: no second checkout.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team" }))).status, 409);

    // A 4th member needs a 4th seat: refused, the owner adds one (charged at once), then it fits.
    let r = add_org_member(&s, &ada, &org, "dee@example.com");
    assert_eq!((r.status, r.json["limit"].as_str(), r.json["max"].as_i64()), (402, Some("seats"), Some(3)));
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 2 }))).status, 400, "never below the seats in use");
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 4 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(4)), "{}", r.json);
    let patch = mock.last("PATCH");
    assert_eq!(patch.path, "/v1/subscription-items/71");
    assert_eq!(patch.body["data"]["attributes"], json!({ "quantity": 4, "invoice_immediately": true }));
    assert_eq!(add_org_member(&s, &ada, &org, "dee@example.com").status, 201);
    // Lemon Squeezy's own update follows; an older one arriving late is stale.
    delivered(&s, &ls::subscription("subscription_updated", "1", ls::TEAM_ANNUAL, "active", 4, "2026-11-03T11:00:01.000000Z", custom.clone()), "applied");
    delivered(&s, &ls::subscription("subscription_updated", "1", ls::TEAM_ANNUAL, "cancelled", 3, "2026-11-03T10:30:00.000000Z", custom.clone()), "stale");
    let b = billing(&s, &ada, &org);
    assert_eq!((b["status"].as_str(), b["seats"].as_i64(), b["seatsUsed"].as_i64()), (Some("active"), Some(4), Some(4)));

    // A workspace with data in it, then a failed renewal: the banner, still Team.
    let ws = "0f0e0d0c0b0a09080706050403020101";
    let key = {
        let key = hitop_sync_crypto::WorkspaceKey::generate();
        let wraps = wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], ws, 1, &key);
        let r = s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": ws, "wraps": wraps, "org": org })));
        assert_eq!(r.status, 201, "{}", r.json);
        key
    };
    let r = s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/a"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": 1 }, "epoch": 1, "content": seal(&key, ws, "a", 1, 1, b"{\"v\":1}") })));
    assert_eq!(r.status, 200, "{}", r.json);
    delivered(&s, &ls::invoice("subscription_payment_failed", "1", false, "2027-11-03T10:00:00.000000Z"), "applied");
    delivered(&s, &ls::subscription("subscription_updated", "1", ls::TEAM_ANNUAL, "past_due", 4, "2027-11-03T10:00:01.000000Z", custom.clone()), "applied");
    let b = billing(&s, &ada, &org);
    assert_eq!((b["pastDue"].as_bool(), b["planInForce"].as_str()), (Some(true), Some("team")));
    // Dunning gives up: expired → Free limits, nothing deleted, every read works.
    delivered(&s, &ls::subscription("subscription_expired", "1", ls::TEAM_ANNUAL, "expired", 4, "2027-11-17T10:00:00.000000Z", custom.clone()), "applied");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["planInForce"].as_str(), l["limits"]["seats"].as_i64()), (Some("free"), Some(3)));
    let r = s.call("GET", &format!("/v1/workspaces/{ws}/blobs/a"), Some(&ada.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(open(&key, ws, "a", 1, 1, r.json["content"].as_str().unwrap()), b"{\"v\":1}");
    assert_eq!(people(&s, &ada, &org), 4, "nobody removed");
    assert_eq!(add_org_member(&s, &ada, &org, "eve@example.com").status, 402, "creates beyond Free refused");
    let over = s.app.state().db.tx(|tx| hitop_sync_server::limits::over_limits(s.app.state(), tx)).unwrap();
    assert!(over.iter().any(|o| o.org == org && o.metric.name() == "seats" && o.used == 4 && o.max == 3), "{over:?}");
    // An expired subscription may be bought again.
    checkout(&s, &mock, &ada, &org, "team", "monthly", Some(4));

    let changes: Vec<String> = audit_rows(&s, &format!("org:{org}")).iter().filter(|r| r["event"] == "plan_changed").map(|r| format!("{}/{}", r["detail"]["to"].as_str().unwrap(), r["detail"]["status"].as_str().unwrap())).collect();
    assert_eq!(changes, ["team/active", "team/active", "team/past_due", "team/expired"], "seats 3 → 4 is the second");
}

#[test]
fn pro_refunds_end_the_plan_until_a_new_payment() {
    let mock = MockLs::start();
    let s = server("billing-refund", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(billing(&s, &ada, &personal)["offers"], json!([{ "plan": "pro", "period": "annual" }, { "plan": "pro", "period": "monthly" }]));
    assert_eq!(s.call("POST", &format!("/v1/orgs/{personal}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team" }))).status, 400);
    let custom = checkout(&s, &mock, &ada, &personal, "pro", "monthly", None);
    assert_eq!(mock.last("POST").body["data"]["attributes"]["checkout_data"]["variant_quantities"][0]["quantity"], 1);
    delivered(&s, &ls::subscription("subscription_created", "5", ls::PRO_MONTHLY, "active", 1, "2026-11-03T10:00:00.000000Z", custom.clone()), "applied");
    let l = limits(&s, &ada, &personal);
    assert_eq!((l["planInForce"].as_str(), l["limits"]["workspaces"].as_i64(), l["limits"]["seats"].as_i64()), (Some("pro"), Some(25), Some(3)), "Pro: one person and two guests");
    // A partial refund changes nothing; a full one ends the plan.
    delivered(&s, &ls::invoice("subscription_payment_refunded", "5", false, "2026-11-04T10:00:00.000000Z"), "ignored");
    delivered(&s, &ls::invoice("subscription_payment_refunded", "5", true, "2026-11-05T10:00:00.000000Z"), "applied");
    assert_eq!(limits(&s, &ada, &personal)["planInForce"], "free");
    // A later subscription update doesn't bring it back; a newer paid invoice does.
    delivered(&s, &ls::subscription("subscription_updated", "5", ls::PRO_MONTHLY, "active", 1, "2026-11-05T10:00:01.000000Z", custom.clone()), "applied");
    assert_eq!(billing(&s, &ada, &personal)["status"], "refunded");
    delivered(&s, &ls::invoice("subscription_payment_success", "5", false, "2026-11-04T09:00:00.000000Z"), "stale");
    delivered(&s, &ls::invoice("subscription_payment_success", "5", false, "2026-12-03T10:00:00.000000Z"), "applied");
    assert_eq!(limits(&s, &ada, &personal)["planInForce"], "pro");
    // The order refunded in full: ended again.
    delivered(&s, &ls::order("order_refunded", "95", true, "2026-12-04T10:00:00.000000Z"), "applied");
    assert_eq!(billing(&s, &ada, &personal)["status"], "refunded");
    // Bought again after the refund (subscription 6); cancelling the refunded one afterwards (as the
    // runbook says) can't take the account back.
    let custom2 = checkout(&s, &mock, &ada, &personal, "pro", "annual", None);
    delivered(&s, &ls::subscription("subscription_created", "6", ls::PRO_ANNUAL, "active", 1, "2026-12-05T10:00:00.000000Z", custom2), "applied");
    delivered(&s, &ls::subscription("subscription_cancelled", "5", ls::PRO_MONTHLY, "cancelled", 1, "2026-12-06T10:00:00.000000Z", custom.clone()), "superseded");
    let b = billing(&s, &ada, &personal);
    assert_eq!((b["status"].as_str(), b["planInForce"].as_str()), (Some("active"), Some("pro")));
    assert_eq!(s.app.state().db.tx(|tx| tx.query_one("SELECT ls_subscription_id FROM entitlements WHERE org_id = $1", &[personal.as_str().into()])).unwrap().unwrap().text(0), "6");
    // An order nobody knows.
    delivered(&s, &ls::order("order_refunded", "12345", true, "2026-12-04T10:00:00.000000Z"), "unmatched");
}

#[test]
fn every_lifecycle_event() {
    let mock = MockLs::start();
    let s = server("billing-lifecycle", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    let custom = checkout(&s, &mock, &ada, &org, "team", "annual", Some(2));
    let at = |n: u32| format!("2026-11-{:02}T10:00:00.000000Z", n);
    let steps: [(&str, &str, &str); 7] = [
        ("subscription_created", "on_trial", "team"),
        ("subscription_updated", "active", "team"),
        ("subscription_paused", "paused", "free"),
        ("subscription_unpaused", "active", "team"),
        ("subscription_cancelled", "cancelled", "team"),
        ("subscription_resumed", "active", "team"),
        ("subscription_expired", "expired", "free"),
    ];
    for (i, (event, status, in_force)) in steps.iter().enumerate() {
        delivered(&s, &ls::subscription(event, "8", ls::TEAM_ANNUAL, status, 2, &at(i as u32 + 3), custom.clone()), "applied");
        let b = billing(&s, &ada, &org);
        assert_eq!((b["status"].as_str(), b["planInForce"].as_str()), (Some(*status), Some(*in_force)), "{event}");
    }
    // Recovered after a failure.
    delivered(&s, &ls::subscription("subscription_resumed", "8", ls::TEAM_ANNUAL, "active", 2, &at(20), custom.clone()), "applied");
    delivered(&s, &ls::invoice("subscription_payment_failed", "8", false, &at(21)), "applied");
    assert!(billing(&s, &ada, &org)["pastDue"].as_bool().unwrap());
    delivered(&s, &ls::invoice("subscription_payment_recovered", "8", false, &at(22)), "applied");
    delivered(&s, &ls::invoice("subscription_payment_success", "8", false, &at(22)), "ignored");
    assert_eq!(billing(&s, &ada, &org)["status"], "active");
    // A monthly/annual switch in the customer portal is a variant change; an unknown variant
    // (not configured here) changes nothing.
    delivered(&s, &ls::subscription("subscription_updated", "8", ls::TEAM_MONTHLY, "active", 2, &at(23), custom.clone()), "applied");
    delivered(&s, &ls::subscription("subscription_updated", "8", "999", "active", 2, &at(24), custom.clone()), "unknown_variant");
    assert_eq!(limits(&s, &ada, &org)["plan"], "team");
    // Every event name the spec lists reached the handler.
    let names: std::collections::BTreeSet<String> = events(&s).into_iter().map(|(e, _)| e).collect();
    for e in ["subscription_created", "subscription_updated", "subscription_cancelled", "subscription_resumed", "subscription_expired", "subscription_paused", "subscription_unpaused", "subscription_payment_success", "subscription_payment_failed", "subscription_payment_recovered"] {
        assert!(names.contains(e), "{e}");
    }
    // The audit chain verifies with the billing events in it.
    assert_eq!(s.call("GET", "/healthz", None, None).status, 200);
    let chains = hitop_sync_server::audit::verify_all(s.app.state()).unwrap();
    assert!(chains.iter().all(|(_, v)| v.problem.is_none()));
}

#[test]
fn which_org_an_event_is_for() {
    let mock = MockLs::start();
    let s = server("billing-match", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let mallory = Device::new(&s, "mallory@example.com");
    let org = org_create(&s, &ada, "Acme");
    let theirs = org_create(&s, &mallory, "Mallory Inc");
    let t = "2026-11-03T10:00:00.000000Z";
    // No custom data (a share link), a forged signature, another org's signature: unmatched.
    delivered(&s, &ls::subscription("subscription_created", "20", ls::TEAM_ANNUAL, "active", 5, t, json!({})), "unmatched");
    delivered(&s, &ls::subscription("subscription_created", "21", ls::TEAM_ANNUAL, "active", 5, t, json!({ "org_id": org, "user_id": ada.user_id, "sig": "0".repeat(64) })), "unmatched");
    let mallorys = checkout(&s, &mock, &mallory, &theirs, "team", "annual", Some(1));
    delivered(&s, &ls::subscription("subscription_created", "22", ls::TEAM_ANNUAL, "active", 5, t, json!({ "org_id": org, "user_id": mallory.user_id, "sig": mallorys["sig"] })), "unmatched");
    assert_eq!(limits(&s, &ada, &org)["plan"], "free");
    // Wrong store, and test mode when the server refuses it, change nothing either.
    let custom = checkout(&s, &mock, &ada, &org, "team", "annual", Some(1));
    let mut other_store = ls::subscription("subscription_created", "23", ls::TEAM_ANNUAL, "active", 5, t, custom.clone());
    other_store["data"]["attributes"]["store_id"] = json!(1);
    delivered(&s, &other_store, "wrong_store");
    let live = {
        let mut c = mock.config();
        c.test_mode = false;
        Server::with("billing-live", |cfg| {
            cfg.plan_limits = true;
            cfg.billing = Some(c);
        })
    };
    delivered(&live, &ls::subscription("subscription_created", "24", ls::TEAM_ANNUAL, "active", 5, t, json!({})), "test_mode_refused");
    // The operator ties an unmatched subscription to its org; its next event applies.
    assert_eq!(s.app.state().db.tx(|tx| tx.query("SELECT outcome FROM billing_events WHERE outcome = 'unmatched'", &[])).unwrap().len(), 3);
    hitop_sync_server::billing::link(s.app.state(), &org, "20").unwrap();
    assert!(hitop_sync_server::billing::link(s.app.state(), &theirs, "20").is_err(), "one org per subscription");
    delivered(&s, &ls::subscription("subscription_updated", "20", ls::TEAM_ANNUAL, "active", 5, "2026-11-03T10:00:01.000000Z", json!({})), "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 5);
    // Another subscription never overrides a running one, lapsed or live (a second purchase shows
    // as superseded in billing-events for the operator to refund).
    delivered(&s, &ls::subscription("subscription_expired", "25", ls::TEAM_ANNUAL, "expired", 2, "2026-11-04T10:00:00.000000Z", custom.clone()), "superseded");
    assert_eq!(billing(&s, &ada, &org)["status"], "active");
    delivered(&s, &ls::invoice("subscription_payment_failed", "25", false, "2026-11-04T10:00:00.000000Z"), "unmatched");
    delivered(&s, &ls::subscription("subscription_created", "26", ls::TEAM_ANNUAL, "active", 7, "2026-11-04T10:00:00.000000Z", custom.clone()), "superseded");
    assert_eq!(limits(&s, &ada, &org)["seats"], 5);
    // Once the org's own has ended, a live one takes over.
    delivered(&s, &ls::subscription("subscription_expired", "20", ls::TEAM_ANNUAL, "expired", 5, "2026-11-05T10:00:00.000000Z", json!({})), "applied");
    delivered(&s, &ls::subscription("subscription_updated", "26", ls::TEAM_ANNUAL, "active", 7, "2026-11-05T11:00:00.000000Z", custom.clone()), "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 7);
    // Bodies are never stored: no buyer name or address on disk.
    let disk = s.disk_bytes();
    assert!(!disk.windows(12).any(|w| w == b"Ada Lovelace"));
    assert!(!disk.windows(15).any(|w| w == b"hitop-test.lemo"), "no signed URLs either");
}

#[test]
fn billing_is_for_owners_and_admins_with_a_session() {
    let mock = MockLs::start();
    let s = server("billing-roles", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let org = org_create(&s, &ada, "Acme");
    assert_eq!(add_org_member(&s, &ada, &org, "bob@example.com").status, 201);
    let stranger = Device::new(&s, "zed@example.com");
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/billing"), Some(&stranger.session), None).status, 404);
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/billing"), Some(&bob.session), None).status, 200, "members read");
    for (m, p, b) in [("POST", "checkout", json!({ "plan": "team" })), ("POST", "seats", json!({ "seats": 3 })), ("POST", "trial", json!({})), ("GET", "portal", Value::Null)] {
        let body = if b.is_null() { None } else { Some(b) };
        assert_eq!(s.call(m, &format!("/v1/orgs/{org}/billing/{p}"), Some(&bob.session), body).status, 403, "{p}");
    }
    let t = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read", "write"] })));
    let pat = t.json["token"].as_str().unwrap();
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(pat), Some(json!({ "plan": "team" })));
    assert_eq!(r.status, 403, "an access token never buys: {}", r.json);
    // The customer portal: a fresh signed link from the API.
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/billing/portal"), Some(&ada.session), None).status, 409, "no subscription yet");
    let custom = checkout(&s, &mock, &ada, &org, "team", "annual", None);
    assert_eq!(mock.last("POST").body["data"]["attributes"]["checkout_data"]["variant_quantities"][0]["quantity"], 2, "default: the seats in use");
    delivered(&s, &ls::subscription("subscription_created", "30", ls::TEAM_ANNUAL, "active", 2, "2026-11-03T10:00:00.000000Z", custom), "applied");
    let r = s.call("GET", &format!("/v1/orgs/{org}/billing/portal"), Some(&ada.session), None);
    assert_eq!((r.status, r.json["url"].as_str()), (200, Some("https://hitop-test.lemonsqueezy.com/billing?sub=30&signature=fresh")));
    // Fewer seats: no credit now, the lower price from the next renewal.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 5 }))).status, 200);
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 3 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(3)));
    assert_eq!(mock.last("PATCH").body["data"]["attributes"], json!({ "quantity": 3, "disable_prorations": true }));
    // No API key: the webhook still works, buying doesn't.
    let mut c = mock.config();
    c.api_key = None;
    let s2 = Server::with("billing-nokey", |cfg| {
        cfg.plan_limits = true;
        cfg.billing = Some(c);
    });
    let d = Device::new(&s2, "ada@example.com");
    let o2 = org_create(&s2, &d, "Acme");
    let r = s2.call("POST", &format!("/v1/orgs/{o2}/billing/checkout"), Some(&d.session), Some(json!({ "plan": "team" })));
    assert_eq!((r.status, r.json["error"].as_str()), (409, Some("billing_api_off")));
    assert_eq!(billing(&s2, &d, &o2)["checkout"], false);
}

#[test]
fn the_team_trial() {
    let mock = MockLs::start();
    let s = server("billing-trial", &mock);
    let ada = Device::new(&s, "ada@acme.io");
    let org = org_create(&s, &ada, "Acme");
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{personal}/billing/trial"), Some(&ada.session), Some(json!({}))).status, 409, "not for a personal org");
    assert!(billing(&s, &ada, &org)["trial"]["available"].as_bool().unwrap());
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/trial"), Some(&ada.session), Some(json!({})));
    assert_eq!((r.status, r.json["seats"].as_i64()), (201, Some(5)), "{}", r.json);
    let l = limits(&s, &ada, &org);
    assert_eq!((l["planInForce"].as_str(), l["status"].as_str(), l["limits"]["seats"].as_i64()), (Some("team"), Some("trialing"), Some(5)));
    for e in ["b@example.com", "c@example.com", "d@example.com", "e@example.com"] {
        assert_eq!(add_org_member(&s, &ada, &org, e).status, 201);
    }
    // One trial per org, per account, per (company) domain.
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/trial"), Some(&ada.session), Some(json!({})));
    assert_eq!(r.status, 409);
    let org2 = org_create(&s, &ada, "Acme 2");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org2}/billing/trial"), Some(&ada.session), Some(json!({}))).status, 409);
    let colleague = Device::new(&s, "zoe@acme.io");
    let org3 = org_create(&s, &colleague, "Acme 3");
    let r = s.call("POST", &format!("/v1/orgs/{org3}/billing/trial"), Some(&colleague.session), Some(json!({})));
    assert_eq!(r.status, 409, "{}", r.json);
    assert!(r.json["message"].as_str().unwrap().contains("domain"));
    let gmail = Device::new(&s, "someone@gmail.com");
    let org4 = org_create(&s, &gmail, "Side project");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org4}/billing/trial"), Some(&gmail.session), Some(json!({}))).status, 201, "free mail isn't a company");
    // Day 15: Free limits, nobody removed, creates beyond Free refused.
    s.advance(15 * 86_400);
    let l = limits(&s, &ada, &org);
    assert_eq!((l["planInForce"].as_str(), l["status"].as_str()), (Some("free"), Some("expired")));
    assert_eq!(people(&s, &ada, &org), 5);
    assert_eq!(add_org_member(&s, &ada, &org, "f@example.com").status, 402);
    let ends: Vec<String> = audit_rows(&s, &format!("org:{org}")).iter().filter(|r| r["event"] == "plan_changed").map(|r| r["detail"]["by"].as_str().unwrap().to_string()).collect();
    assert_eq!(ends, ["trial", "trial_end"]);
    // Bought after the trial: the subscription replaces it.
    let custom = checkout(&s, &mock, &ada, &org, "team", "annual", Some(5));
    delivered(&s, &ls::subscription("subscription_created", "40", ls::TEAM_ANNUAL, "active", 6, "2026-11-20T10:00:00.000000Z", custom), "applied");
    assert_eq!(add_org_member(&s, &ada, &org, "f@example.com").status, 201);
}

#[test]
fn business_is_sold_per_seat_once_its_variants_are_set() {
    use hitop_sync_server::billing::Variant;
    let mock = MockLs::start();
    // Without Business variants (C3's setup), Business isn't on sale.
    let s = server("billing-business-off", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "business", "seats": 1 }))).status, 400);

    let mut c = mock.config();
    c.variants.push(Variant { plan: "business", period: "annual", id: ls::BUSINESS_ANNUAL.into() });
    c.variants.push(Variant { plan: "business", period: "monthly", id: ls::BUSINESS_MONTHLY.into() });
    let s = Server::with("billing-business", |cfg| {
        cfg.plan_limits = true;
        cfg.billing = Some(c);
    });
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    assert_eq!(add_org_member(&s, &ada, &org, "bob@example.com").status, 201);
    let offers = billing(&s, &ada, &org)["offers"].clone();
    assert!(offers.as_array().unwrap().contains(&json!({ "plan": "business", "period": "annual" })), "{offers}");
    // A personal account still buys Pro only.
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{personal}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "business" }))).status, 400);

    let custom = checkout(&s, &mock, &ada, &org, "business", "annual", Some(4));
    let sent = mock.last("POST").body;
    assert_eq!(sent["data"]["attributes"]["checkout_data"]["variant_quantities"], json!([{ "variant_id": 301, "quantity": 4 }]));
    delivered(&s, &ls::subscription("subscription_created", "9", ls::BUSINESS_ANNUAL, "active", 4, "2026-11-03T10:00:00.000000Z", custom), "applied");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["planInForce"].as_str(), l["seats"].as_i64(), l["limits"]["seats"].as_i64()), (Some("business"), Some(4), Some(4)));
    // Business features open: the audit export, the admin console's actions.
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/audit/export"), Some(&ada.session), None).status, 200);
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/admin"), Some(&ada.session), None).json["actions"]["removeDevice"], true);
    // Seats change through the API as for Team.
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 5 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(5)), "{}", r.json);
}
