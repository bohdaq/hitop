//! Billing (HITOP Cloud C3, billing.rs): Stripe Managed Payments webhook deliveries built from the
//! documented event shapes (fixtures/stripe/) against a server with plan limits on and a fake
//! Stripe API (common/stripe.rs). Every event billing.rs handles; a bad or old signature refused
//! before the body is read; a replay changing nothing; out-of-order events; test events on a live
//! key; Team bought with 3 seats, a 4th member after buying a seat; payment failed → banner,
//! deleted → Free with every byte still readable; refunds ending the plan; the trial; checkouts
//! that sign the org.

mod common;

use common::stripe::{self as st, delivered, deliver, MockStripe, T0};
use common::*;
use serde_json::{json, Value};

fn server(name: &str, mock: &MockStripe) -> Server {
    let c = mock.config();
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

/// A checkout through the fake API; returns the metadata it signed (what Stripe then sends back
/// on the session and the subscription of that purchase).
fn checkout(s: &Server, mock: &MockStripe, d: &Device, org: &str, plan: &str, period: &str, seats: Option<i64>) -> Value {
    let mut body = json!({ "plan": plan, "period": period });
    if let Some(n) = seats {
        body["seats"] = json!(n);
    }
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&d.session), Some(body));
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(r.json["url"].as_str().unwrap().starts_with("https://checkout.stripe.com/"));
    let seen = mock.last("/v1/checkout/sessions");
    assert!(seen.authorized, "secret key and pinned Stripe-Version");
    assert_eq!(st::refused_keys(&seen), Vec::<String>::new(), "nothing Managed Payments refuses");
    let meta = seen.metadata("metadata");
    assert_eq!(seen.metadata("subscription_data[metadata]"), meta, "the subscription carries the same signed metadata");
    assert_eq!(seen.get("client_reference_id"), Some(org));
    meta
}

fn events(s: &Server) -> Vec<(String, String)> {
    s.app.state().db.tx(|tx| tx.query("SELECT event_name, outcome FROM billing_events ORDER BY received_at, id", &[])).unwrap().iter().map(|r| (r.text(0), r.text(1))).collect()
}

#[test]
fn off_unless_configured() {
    let s = Server::with("billing-off", |c| c.plan_limits = true);
    let d = Device::new(&s, "ada@example.com");
    let body = st::subscription("customer.subscription.created", "sub_1", st::TEAM_ANNUAL, "active", 3, T0, json!({}));
    assert_eq!(deliver(&s, &body).status, 404);
    let org = org_create(&s, &d, "Acme");
    let b = billing(&s, &d, &org);
    assert_eq!((b["enabled"].as_bool(), b["checkout"].as_bool(), b["offers"].as_array().unwrap().len()), (Some(false), Some(false), 0));
    assert_eq!(b["trial"]["available"], false, "trials open with billing");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/trial"), Some(&d.session), Some(json!({}))).status, 409);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&d.session), Some(json!({ "plan": "team" }))).status, 404);
    // The old provider's route is gone.
    assert_eq!(s.raw("POST", "/v1/billing/lemonsqueezy", None, b"{}".to_vec(), &[]).status, 404);
}

#[test]
fn a_bad_signature_is_refused_before_the_body_is_read() {
    let mock = MockStripe::start();
    let s = server("billing-sig", &mock);
    let body = st::subscription("customer.subscription.created", "sub_1", st::TEAM_ANNUAL, "active", 3, T0, json!({}));
    let bytes = body.to_string().into_bytes();
    let now = s.app.state().now();
    let sign = |t: i64, b: &[u8]| hitop_sync_server::billing::stripe_signature(st::WEBHOOK_SECRET, t, b);
    let good = sign(now, &bytes);
    for sig in [
        None,
        Some(format!("t={now},v1={}", "0".repeat(64))),
        Some(format!("t={now},v1={}", sign(now, b"another body"))),
        Some(format!("t={now},v1={}", &good[..60])),
        Some(format!("v1={good}")),
        Some(format!("t={},v1={}", now - 301, sign(now - 301, &bytes))),
        Some(format!("t={now},v1={}", hitop_sync_server::billing::stripe_signature("whsec_other", now, &bytes))),
    ] {
        let mut headers = vec![("Content-Type", "application/json")];
        if let Some(s) = &sig {
            headers.push(("Stripe-Signature", s.as_str()));
        }
        let r = s.raw("POST", "/v1/billing/stripe", None, bytes.clone(), &headers);
        assert_eq!((r.status, r.json["error"].as_str()), (401, Some("bad_signature")), "{sig:?}");
    }
    // Not JSON, unsigned: still 401, not a parse error (nothing is parsed first).
    let r = s.raw("POST", "/v1/billing/stripe", None, b"not json".to_vec(), &[("Stripe-Signature", "t=1,v1=x")]);
    assert_eq!(r.status, 401);
    // Signed garbage is a 400; so is signed JSON that isn't an event.
    let r = s.raw("POST", "/v1/billing/stripe", None, b"not json".to_vec(), &[("Stripe-Signature", &st::signature(&s, b"not json"))]);
    assert_eq!(r.status, 400);
    let r = s.raw("POST", "/v1/billing/stripe", None, b"{\"id\":\"x\"}".to_vec(), &[("Stripe-Signature", &st::signature(&s, b"{\"id\":\"x\"}"))]);
    assert_eq!(r.status, 400);
    assert!(events(&s).is_empty(), "nothing recorded");
    // A second v1 (the secret being rolled) with the right one among them: accepted (unmatched:
    // it names no org).
    let r = s.raw("POST", "/v1/billing/stripe", None, bytes.clone(), &[("Stripe-Signature", &format!("t={now},v1={},v1={good}", "1".repeat(64)))]);
    assert_eq!((r.status, r.json["outcome"].as_str()), (200, Some("unmatched")), "{}", r.json);
}

#[test]
fn team_with_three_seats_a_fourth_seat_then_payment_failed_and_deleted() {
    let mock = MockStripe::start();
    let s = server("billing-team", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    for e in ["bob@example.com", "cy@example.com"] {
        assert_eq!(add_org_member(&s, &ada, &org, e).status, 201);
    }
    let b = billing(&s, &ada, &org);
    assert_eq!((b["enabled"].as_bool(), b["checkout"].as_bool(), b["testMode"].as_bool(), b["seatsUsed"].as_i64()), (Some(true), Some(true), Some(true), Some(3)));
    assert_eq!(b["offers"], json!([{ "plan": "team", "period": "annual" }, { "plan": "team", "period": "monthly" }]));
    // Free: a 4th person is refused.
    assert_eq!(add_org_member(&s, &ada, &org, "dee@example.com").status, 402);
    // Pro isn't for a team org; fewer seats than used are refused.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "pro" }))).status, 400);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team", "seats": 2 }))).status, 400);

    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", Some(3));
    let sent = mock.last("/v1/checkout/sessions");
    assert_eq!((sent.get("managed_payments[enabled]"), sent.get("mode")), (Some("true"), Some("subscription")), "Managed Payments on");
    assert_eq!((sent.get("line_items[0][price]"), sent.get("line_items[0][quantity]")), (Some(st::TEAM_ANNUAL), Some("3")));
    assert_eq!(sent.get("customer_email"), Some("ada@example.com"));
    assert_eq!(sent.get("success_url"), sent.get("cancel_url"), "back to the plan page either way");
    assert!(sent.get("success_url").unwrap().ends_with(&format!("#/orgs/{org}/plan")));
    assert_eq!((meta["org_id"].as_str(), meta["user_id"].as_str()), (Some(org.as_str()), Some(ada.user_id.as_str())));
    assert_eq!(meta["sig"].as_str().unwrap().len(), 64);

    // The purchase: the subscription first (Stripe doesn't order deliveries), the session, the
    // first invoice paid, all in one second.
    let created = st::subscription("customer.subscription.created", "sub_1", st::TEAM_ANNUAL, "active", 3, T0, meta.clone());
    delivered(&s, &created, "applied");
    delivered(&s, &st::session("sub_1", &st::customer_of("sub_1"), meta.clone(), T0), "applied");
    delivered(&s, &st::invoice("invoice.paid", "sub_1", T0), "ignored");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["plan"].as_str(), l["planInForce"].as_str(), l["seats"].as_i64(), l["limits"]["seats"].as_i64(), l["limits"]["guestViewers"].as_i64()), (Some("team"), Some("team"), Some(3), Some(3), Some(5)));
    // A replay of the same event (same evt_ id) changes nothing.
    delivered(&s, &created, "duplicate");
    // Already subscribed: no second checkout.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team" }))).status, 409);

    // A 4th member needs a 4th seat: refused, the owner adds one (invoiced at once), then it fits.
    let r = add_org_member(&s, &ada, &org, "dee@example.com");
    assert_eq!((r.status, r.json["limit"].as_str(), r.json["max"].as_i64()), (402, Some("seats"), Some(3)));
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 2 }))).status, 400, "never below the seats in use");
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 4 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(4)), "{}", r.json);
    let update = mock.last("/v1/subscriptions/");
    assert_eq!(update.path, "/v1/subscriptions/sub_1");
    assert_eq!((update.get("items[0][id]"), update.get("items[0][quantity]")), (Some("si_sub_1"), Some("4")));
    assert_eq!((update.get("proration_behavior"), update.get("payment_behavior")), (Some("always_invoice"), Some("error_if_incomplete")));
    assert_eq!(add_org_member(&s, &ada, &org, "dee@example.com").status, 201);
    // A declined proration charge: Stripe's 402, nothing changes here.
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": st::DECLINED_SEATS })));
    assert_eq!((r.status, r.json["error"].as_str()), (502, Some("billing_refused")), "{}", r.json);
    assert!(r.json["message"].as_str().unwrap().contains("declined"));
    assert_eq!(billing(&s, &ada, &org)["seats"], 4);
    // Stripe's own update follows; an older one arriving late is stale.
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_1", st::TEAM_ANNUAL, "active", 4, T0 + 3601, meta.clone()), "applied");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_1", st::TEAM_ANNUAL, "canceled", 3, T0 + 1800, meta.clone()), "stale");
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
    let year = 365 * 86_400;
    delivered(&s, &st::invoice("invoice.payment_failed", "sub_1", T0 + year), "applied");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_1", st::TEAM_ANNUAL, "past_due", 4, T0 + year + 1, meta.clone()), "applied");
    let b = billing(&s, &ada, &org);
    assert_eq!((b["pastDue"].as_bool(), b["planInForce"].as_str()), (Some(true), Some("team")));
    // Dunning gives up and cancels: deleted → Free limits, nothing deleted, every read works.
    delivered(&s, &st::subscription("customer.subscription.deleted", "sub_1", st::TEAM_ANNUAL, "canceled", 4, T0 + year + 14 * 86_400, meta.clone()), "applied");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["status"].as_str(), l["planInForce"].as_str(), l["limits"]["seats"].as_i64()), (Some("expired"), Some("free"), Some(3)));
    let r = s.call("GET", &format!("/v1/workspaces/{ws}/blobs/a"), Some(&ada.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(open(&key, ws, "a", 1, 1, r.json["content"].as_str().unwrap()), b"{\"v\":1}");
    assert_eq!(people(&s, &ada, &org), 4, "nobody removed");
    assert_eq!(add_org_member(&s, &ada, &org, "eve@example.com").status, 402, "creates beyond Free refused");
    let over = s.app.state().db.tx(|tx| hitop_sync_server::limits::over_limits(s.app.state(), tx)).unwrap();
    assert!(over.iter().any(|o| o.org == org && o.metric.name() == "seats" && o.used == 4 && o.max == 3), "{over:?}");
    // An ended subscription may be bought again.
    checkout(&s, &mock, &ada, &org, "team", "monthly", Some(4));

    let changes: Vec<String> = audit_rows(&s, &format!("org:{org}")).iter().filter(|r| r["event"] == "plan_changed").map(|r| format!("{}/{}", r["detail"]["to"].as_str().unwrap(), r["detail"]["status"].as_str().unwrap())).collect();
    assert_eq!(changes, ["team/active", "team/active", "team/past_due", "team/expired"], "seats 3 → 4 is the second");
}

#[test]
fn pro_refunds_end_the_plan_until_a_new_payment() {
    let mock = MockStripe::start();
    let s = server("billing-refund", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(billing(&s, &ada, &personal)["offers"], json!([{ "plan": "pro", "period": "annual" }, { "plan": "pro", "period": "monthly" }]));
    assert_eq!(s.call("POST", &format!("/v1/orgs/{personal}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "team" }))).status, 400);
    let meta = checkout(&s, &mock, &ada, &personal, "pro", "monthly", None);
    assert_eq!(mock.last("/v1/checkout/sessions").get("line_items[0][quantity]"), Some("1"));
    delivered(&s, &st::subscription("customer.subscription.created", "sub_5", st::PRO_MONTHLY, "active", 1, T0, meta.clone()), "applied");
    let l = limits(&s, &ada, &personal);
    assert_eq!((l["planInForce"].as_str(), l["limits"]["workspaces"].as_i64(), l["limits"]["seats"].as_i64()), (Some("pro"), Some(25), Some(3)), "Pro: one person and two guests");
    // A partial refund changes nothing; a full one ends the plan.
    let customer = st::customer_of("sub_5");
    delivered(&s, &st::refund(&customer, false, T0 + 86_400), "ignored");
    delivered(&s, &st::refund(&customer, true, T0 + 2 * 86_400), "applied");
    assert_eq!(limits(&s, &ada, &personal)["planInForce"], "free");
    // A later subscription update doesn't bring it back; a newer paid invoice does.
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_5", st::PRO_MONTHLY, "active", 1, T0 + 2 * 86_400 + 1, meta.clone()), "applied");
    assert_eq!(billing(&s, &ada, &personal)["status"], "refunded");
    delivered(&s, &st::invoice("invoice.paid", "sub_5", T0 + 86_400), "stale");
    delivered(&s, &st::invoice("invoice.paid", "sub_5", T0 + 30 * 86_400), "applied");
    assert_eq!(limits(&s, &ada, &personal)["planInForce"], "pro");
    // Refunded again, then resumed by the owner in the Dashboard: lifted too.
    delivered(&s, &st::refund(&customer, true, T0 + 31 * 86_400), "applied");
    delivered(&s, &st::subscription("customer.subscription.resumed", "sub_5", st::PRO_MONTHLY, "active", 1, T0 + 31 * 86_400 + 5, meta.clone()), "applied");
    assert_eq!(billing(&s, &ada, &personal)["status"], "active");
    delivered(&s, &st::refund(&customer, true, T0 + 32 * 86_400), "applied");
    assert_eq!(billing(&s, &ada, &personal)["status"], "refunded");
    // Bought again after the refund (sub_6); cancelling the refunded one afterwards (as the
    // runbook says) can't take the account back.
    let meta2 = checkout(&s, &mock, &ada, &personal, "pro", "annual", None);
    delivered(&s, &st::subscription("customer.subscription.created", "sub_6", st::PRO_ANNUAL, "active", 1, T0 + 33 * 86_400, meta2.clone()), "applied");
    delivered(&s, &st::session("sub_6", &st::customer_of("sub_6"), meta2, T0 + 33 * 86_400), "applied");
    delivered(&s, &st::subscription("customer.subscription.deleted", "sub_5", st::PRO_MONTHLY, "canceled", 1, T0 + 34 * 86_400, meta.clone()), "superseded");
    let b = billing(&s, &ada, &personal);
    assert_eq!((b["status"].as_str(), b["planInForce"].as_str()), (Some("active"), Some("pro")));
    let ids = s.app.state().db.tx(|tx| tx.query_one("SELECT ls_subscription_id, ls_customer_id, ls_variant_id, ls_order_id FROM entitlements WHERE org_id = $1", &[personal.as_str().into()])).unwrap().unwrap();
    assert_eq!((ids.text(0), ids.text(1), ids.text(2), ids.text(3)), ("sub_6".to_string(), st::customer_of("sub_6"), st::PRO_ANNUAL.to_string(), "cs_test_a1hitop".to_string()));
    // A refund of a charge nobody here knows.
    delivered(&s, &st::refund("cus_somebody", true, T0 + 35 * 86_400), "unmatched");
}

#[test]
fn every_lifecycle_event() {
    let mock = MockStripe::start();
    let s = server("billing-lifecycle", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", Some(2));
    let at = |n: i64| T0 + n * 86_400;
    let steps: [(&str, &str, &str, &str); 6] = [
        ("customer.subscription.created", "trialing", "on_trial", "team"),
        ("customer.subscription.updated", "active", "active", "team"),
        ("customer.subscription.paused", "paused", "paused", "free"),
        ("customer.subscription.resumed", "active", "active", "team"),
        ("customer.subscription.updated", "unpaid", "unpaid", "free"),
        ("customer.subscription.updated", "incomplete", "incomplete", "free"),
    ];
    for (i, (event, stripe, ours, in_force)) in steps.iter().enumerate() {
        delivered(&s, &st::subscription(event, "sub_8", st::TEAM_ANNUAL, stripe, 2, at(i as i64 + 1), meta.clone()), "applied");
        let b = billing(&s, &ada, &org);
        assert_eq!((b["status"].as_str(), b["planInForce"].as_str()), (Some(*ours), Some(*in_force)), "{event} {stripe}");
    }
    // Recovered, then a failure and a paid retry.
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_8", st::TEAM_ANNUAL, "active", 2, at(10), meta.clone()), "applied");
    delivered(&s, &st::invoice("invoice.payment_failed", "sub_8", at(11)), "applied");
    assert!(billing(&s, &ada, &org)["pastDue"].as_bool().unwrap());
    delivered(&s, &st::invoice("invoice.paid", "sub_8", at(12)), "applied");
    assert_eq!(billing(&s, &ada, &org)["status"], "active");
    // A monthly/annual switch in the customer portal is a price change; an unknown price (not
    // configured here) changes nothing.
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_8", st::TEAM_MONTHLY, "active", 2, at(13), meta.clone()), "applied");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_8", "price_somebody_else", "active", 2, at(14), meta.clone()), "unknown_price");
    assert_eq!(limits(&s, &ada, &org)["plan"], "team");
    // Cancelled in the portal at the period's end: the plan stays until then, then Free (by the
    // server's own clock even if Stripe's `deleted` is late), and `deleted` changes nothing more.
    let end = s.app.state().now() + 10 * 86_400;
    delivered(&s, &st::cancelling("sub_8", st::TEAM_MONTHLY, 2, at(15), end, meta.clone()), "applied");
    let b = billing(&s, &ada, &org);
    assert_eq!((b["status"].as_str(), b["planInForce"].as_str(), b["periodEnd"].as_i64()), (Some("cancelled"), Some("team"), Some(end)));
    assert_eq!(b["subscribed"], true, "change it in the portal, not a second checkout");
    s.advance(11 * 86_400);
    let b = billing(&s, &ada, &org);
    assert_eq!((b["status"].as_str(), b["planInForce"].as_str(), b["subscribed"].as_bool()), (Some("expired"), Some("free"), Some(false)));
    delivered(&s, &st::subscription("customer.subscription.deleted", "sub_8", st::TEAM_MONTHLY, "canceled", 2, at(26), meta.clone()), "applied");
    assert_eq!(billing(&s, &ada, &org)["status"], "expired");
    // Events nobody asked for are recorded and ignored.
    let mut other = st::invoice("invoice.finalized", "sub_8", at(27));
    other["data"]["object"]["status"] = json!("open");
    delivered(&s, &other, "ignored");
    // Every event type billing.rs handles reached the handler.
    let names: std::collections::BTreeSet<String> = events(&s).into_iter().map(|(e, _)| e).collect();
    for e in ["customer.subscription.created", "customer.subscription.updated", "customer.subscription.deleted", "customer.subscription.paused", "customer.subscription.resumed", "invoice.paid", "invoice.payment_failed"] {
        assert!(names.contains(e), "{e}");
    }
    // The audit chain verifies with the billing events in it.
    assert_eq!(s.call("GET", "/healthz", None, None).status, 200);
    let chains = hitop_sync_server::audit::verify_all(s.app.state()).unwrap();
    assert!(chains.iter().all(|(_, v)| v.problem.is_none()));
}

#[test]
fn which_org_an_event_is_for() {
    let mock = MockStripe::start();
    let s = server("billing-match", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let mallory = Device::new(&s, "mallory@example.com");
    let org = org_create(&s, &ada, "Acme");
    let theirs = org_create(&s, &mallory, "Mallory Inc");
    // No metadata (a Payment Link), a forged signature, another org's signature: unmatched.
    delivered(&s, &st::subscription("customer.subscription.created", "sub_20", st::TEAM_ANNUAL, "active", 5, T0, json!({})), "unmatched");
    delivered(&s, &st::subscription("customer.subscription.created", "sub_21", st::TEAM_ANNUAL, "active", 5, T0, json!({ "org_id": org, "user_id": ada.user_id, "sig": "0".repeat(64) })), "unmatched");
    let mallorys = checkout(&s, &mock, &mallory, &theirs, "team", "annual", Some(1));
    delivered(&s, &st::subscription("customer.subscription.created", "sub_22", st::TEAM_ANNUAL, "active", 5, T0, json!({ "org_id": org, "user_id": mallory.user_id, "sig": mallorys["sig"] })), "unmatched");
    delivered(&s, &st::session("sub_22", "cus_22", json!({ "org_id": org, "user_id": mallory.user_id, "sig": mallorys["sig"] }), T0), "unmatched");
    assert_eq!(limits(&s, &ada, &org)["plan"], "free");
    // A test-mode event on a server with a live key changes nothing.
    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", Some(1));
    let live = {
        let mut c = mock.config();
        c.live = true;
        Server::with("billing-live", |cfg| {
            cfg.plan_limits = true;
            cfg.billing = Some(c);
        })
    };
    delivered(&live, &st::subscription("customer.subscription.created", "sub_24", st::TEAM_ANNUAL, "active", 5, T0, json!({})), "livemode_mismatch");
    // ...and a live event on a test key neither.
    let mut live_event = st::subscription("customer.subscription.created", "sub_23", st::TEAM_ANNUAL, "active", 5, T0, meta.clone());
    live_event["livemode"] = json!(true);
    delivered(&s, &live_event, "livemode_mismatch");
    assert_eq!(limits(&s, &ada, &org)["plan"], "free");
    // The operator ties an unmatched subscription to its org; its next event applies.
    assert_eq!(s.app.state().db.tx(|tx| tx.query("SELECT outcome FROM billing_events WHERE outcome = 'unmatched'", &[])).unwrap().len(), 4);
    assert_eq!(hitop_sync_server::billing::list_events(s.app.state(), true).unwrap().len(), 4);
    hitop_sync_server::billing::link(s.app.state(), &org, "sub_20").unwrap();
    assert!(hitop_sync_server::billing::link(s.app.state(), &theirs, "sub_20").is_err(), "one org per subscription");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_20", st::TEAM_ANNUAL, "active", 5, T0 + 1, json!({})), "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 5);
    // Another subscription never overrides a running one, lapsed or live (a second purchase shows
    // as superseded in billing-events for the operator to refund).
    delivered(&s, &st::subscription("customer.subscription.deleted", "sub_25", st::TEAM_ANNUAL, "canceled", 2, T0 + 86_400, meta.clone()), "superseded");
    assert_eq!(billing(&s, &ada, &org)["status"], "active");
    delivered(&s, &st::invoice("invoice.payment_failed", "sub_25", T0 + 86_400), "unmatched");
    delivered(&s, &st::subscription("customer.subscription.created", "sub_26", st::TEAM_ANNUAL, "active", 7, T0 + 86_400, meta.clone()), "superseded");
    delivered(&s, &st::session("sub_26", "cus_26", meta.clone(), T0 + 86_400), "superseded");
    assert_eq!(limits(&s, &ada, &org)["seats"], 5);
    // Once the org's own has ended, a live one takes over.
    delivered(&s, &st::subscription("customer.subscription.deleted", "sub_20", st::TEAM_ANNUAL, "canceled", 5, T0 + 2 * 86_400, json!({})), "applied");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_26", st::TEAM_ANNUAL, "active", 7, T0 + 2 * 86_400 + 60, meta.clone()), "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 7);
    // Several events in one second: the later delivery wins.
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_26", st::TEAM_ANNUAL, "active", 8, T0 + 3 * 86_400, meta.clone()), "applied");
    delivered(&s, &st::subscription("customer.subscription.updated", "sub_26", st::TEAM_ANNUAL, "active", 9, T0 + 3 * 86_400, meta.clone()), "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 9);
    // Bodies are never stored: no buyer name or address on disk, no checkout URL either.
    let disk = s.disk_bytes();
    assert!(!disk.windows(12).any(|w| w == b"Ada Lovelace"));
    assert!(!disk.windows(17).any(|w| w == b"St James's Square"));
    assert!(!disk.windows(19).any(|w| w == b"checkout.stripe.com"), "no signed URLs either");
}

#[test]
fn billing_is_for_owners_and_admins_with_a_session() {
    let mock = MockStripe::start();
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
    // The customer portal: a fresh session from the API for the org's customer.
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/billing/portal"), Some(&ada.session), None).status, 409, "no subscription yet");
    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", None);
    assert_eq!(mock.last("/v1/checkout/sessions").get("line_items[0][quantity]"), Some("2"), "default: the seats in use");
    delivered(&s, &st::subscription("customer.subscription.created", "sub_30", st::TEAM_ANNUAL, "active", 2, T0, meta), "applied");
    let r = s.call("GET", &format!("/v1/orgs/{org}/billing/portal"), Some(&ada.session), None);
    assert_eq!((r.status, r.json["url"].as_str()), (200, Some("https://billing.stripe.com/p/session/test_cus_30")), "{}", r.json);
    let seen = mock.last("/v1/billing_portal/sessions");
    assert!(seen.authorized && seen.get("return_url").unwrap().ends_with(&format!("#/orgs/{org}/plan")));
    // Fewer seats: no credit now, the lower price from the next renewal.
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 5 }))).status, 200);
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 3 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(3)));
    let update = mock.last("/v1/subscriptions/");
    assert_eq!((update.get("items[0][quantity]"), update.get("proration_behavior"), update.get("payment_behavior")), (Some("3"), Some("none"), None));
    // A wrong key: Stripe's 401 becomes billing_refused, nothing changes.
    let mut c = mock.config();
    c.secret_key = "sk_test_wrong".into();
    let s2 = Server::with("billing-badkey", |cfg| {
        cfg.plan_limits = true;
        cfg.billing = Some(c);
    });
    let d = Device::new(&s2, "ada@example.com");
    let o2 = org_create(&s2, &d, "Acme");
    let r = s2.call("POST", &format!("/v1/orgs/{o2}/billing/checkout"), Some(&d.session), Some(json!({ "plan": "team" })));
    assert_eq!((r.status, r.json["error"].as_str()), (502, Some("billing_refused")));
}

#[test]
fn the_team_trial() {
    let mock = MockStripe::start();
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
    // Bought after the trial (signed with the server's clock, 15 days on): the subscription
    // replaces it.
    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", Some(5));
    delivered(&s, &st::subscription("customer.subscription.created", "sub_40", st::TEAM_ANNUAL, "active", 6, T0 + 17 * 86_400, meta), "applied");
    assert_eq!(add_org_member(&s, &ada, &org, "f@example.com").status, 201);
}

#[test]
fn business_is_sold_per_seat_once_its_prices_are_set() {
    use hitop_sync_server::billing::Price;
    let mock = MockStripe::start();
    // Without Business prices, Business isn't on sale.
    let s = server("billing-business-off", &mock);
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "business", "seats": 1 }))).status, 400);

    let mut c = mock.config();
    c.prices.push(Price { plan: "business", period: "annual", id: st::BUSINESS_ANNUAL.into() });
    c.prices.push(Price { plan: "business", period: "monthly", id: st::BUSINESS_MONTHLY.into() });
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

    let meta = checkout(&s, &mock, &ada, &org, "business", "annual", Some(4));
    let sent = mock.last("/v1/checkout/sessions");
    assert_eq!((sent.get("line_items[0][price]"), sent.get("line_items[0][quantity]")), (Some(st::BUSINESS_ANNUAL), Some("4")));
    delivered(&s, &st::subscription("customer.subscription.created", "sub_9", st::BUSINESS_ANNUAL, "active", 4, T0, meta), "applied");
    let l = limits(&s, &ada, &org);
    assert_eq!((l["planInForce"].as_str(), l["seats"].as_i64(), l["limits"]["seats"].as_i64()), (Some("business"), Some(4), Some(4)));
    // Business features open: the audit export, the admin console's actions.
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/audit/export"), Some(&ada.session), None).status, 200);
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/admin"), Some(&ada.session), None).json["actions"]["removeDevice"], true);
    // Seats change through the API as for Team.
    let r = s.call("POST", &format!("/v1/orgs/{org}/billing/seats"), Some(&ada.session), Some(json!({ "seats": 5 })));
    assert_eq!((r.status, r.json["seats"].as_i64()), (200, Some(5)), "{}", r.json);
}

#[test]
fn prices_by_lookup_key_and_nothing_managed_payments_refuses() {
    let mock = MockStripe::start();
    let c = mock.config_by_lookup_key();
    let s = Server::with("billing-lookup", |cfg| {
        cfg.plan_limits = true;
        cfg.billing = Some(c);
    });
    let ada = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &ada, "Acme");
    // What the fake lists by lookup key is on sale (no Business there); asked once, then cached.
    assert_eq!(billing(&s, &ada, &org)["offers"], json!([{ "plan": "team", "period": "annual" }, { "plan": "team", "period": "monthly" }]));
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(billing(&s, &ada, &personal)["offers"], json!([{ "plan": "pro", "period": "annual" }, { "plan": "pro", "period": "monthly" }]));
    let asked = mock.last("/v1/prices");
    assert!(asked.authorized);
    let keys: Vec<&str> = asked.form.iter().filter(|(k, _)| k == "lookup_keys[]").map(|(_, v)| v.as_str()).collect();
    assert_eq!(keys, ["pro_yearly", "pro_monthly", "team_yearly", "team_monthly", "business_yearly", "business_monthly"]);
    let meta = checkout(&s, &mock, &ada, &org, "team", "annual", Some(2));
    let sent = mock.last("/v1/checkout/sessions");
    assert_eq!(sent.get("line_items[0][price]"), Some(st::listed_id("team_yearly").as_str()));
    // Every key the request sent, against the list Managed Payments refuses.
    for (k, _) in &sent.form {
        assert!(!st::REFUSED_BY_MANAGED_PAYMENTS.iter().any(|r| k.starts_with(r)), "{k}");
    }
    assert_eq!(mock.count("GET", "/v1/prices"), 1, "cached");
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/billing/checkout"), Some(&ada.session), Some(json!({ "plan": "business", "seats": 2 }))).status, 400, "no Business price listed");
    // The webhook knows the plan by the price id it looked up, or by the lookup key the event's
    // price carries (a price made after the cache was filled).
    delivered(&s, &st::subscription("customer.subscription.created", "sub_50", &st::listed_id("team_yearly"), "active", 2, T0, meta.clone()), "applied");
    assert_eq!(limits(&s, &ada, &org)["planInForce"], "team");
    let mut newer = st::subscription("customer.subscription.updated", "sub_50", "price_brand_new", "active", 3, T0 + 60, meta.clone());
    newer["data"]["object"]["items"]["data"][0]["price"]["lookup_key"] = json!("team_monthly");
    delivered(&s, &newer, "applied");
    assert_eq!(limits(&s, &ada, &org)["seats"], 3);
    // After 10 minutes the list is asked again.
    s.advance(601);
    billing(&s, &ada, &org);
    assert_eq!(mock.count("GET", "/v1/prices"), 2);
}
