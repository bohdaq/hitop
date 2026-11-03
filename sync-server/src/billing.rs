//! Billing for HITOP Cloud (spec/CLOUD_SERVICE.md section 6, tranche C3): Lemon Squeezy as the
//! merchant of record, a 14-day Team trial run by this server, and the endpoints the portal uses.
//! Off unless configured (a self-hosted server sells nothing): every endpoint here answers 404
//! `billing_off` without `HITOP_SYNC_LS_WEBHOOK_SECRET`.
//!
//! Settings:
//!
//! - `HITOP_SYNC_LS_WEBHOOK_SECRET` (or `_FILE`): the webhook's signing secret. Turns billing on.
//! - `HITOP_SYNC_LS_STORE_ID`: the store; events of any other store are recorded and ignored.
//! - `HITOP_SYNC_LS_VARIANT_PRO_ANNUAL`, `_PRO_MONTHLY`, `_TEAM_ANNUAL`, `_TEAM_MONTHLY`: the
//!   variant ids (at least one). The variant decides the plan; prices live in Lemon Squeezy.
//! - `HITOP_SYNC_LS_API_KEY` (or `_FILE`): for checkouts, seat changes and customer portal links.
//!   Without it the webhook still updates plans, and those three answer 409 `billing_api_off`.
//! - `HITOP_SYNC_LS_TEST_MODE`: `off` (default) refuses test-mode events (a test purchase must
//!   never grant a live plan); `on` accepts them (the store in test mode, before launch).
//! - `HITOP_SYNC_LS_API_URL`: the API (default `https://api.lemonsqueezy.com`; tests point it at
//!   a fake).
//! - `HITOP_SYNC_TRIAL_DAYS`: the Team trial (default 14; 0 = no trials). Trials need
//!   `HITOP_SYNC_LIMITS=plans`, not Lemon Squeezy.
//!
//! **Webhook** (`POST /v1/billing/lemonsqueezy`). The `X-Signature` header must be the hex
//! HMAC-SHA256 of the raw body under the signing secret (compared in constant time) before the
//! body is parsed: anything else is a 401 and nothing is read. Lemon Squeezy's body has no event
//! id and its retries resend the same bytes, so a delivery's id is the SHA-256 of the body: one
//! already seen answers 200 `duplicate` and changes nothing. Bodies are never stored (they hold
//! the buyer's name, address and card brand); `billing_events` keeps the id, event name, org,
//! subscription id, the resource's `updated_at` and the outcome. Every verified delivery answers
//! 200 with its outcome (`applied`, `duplicate`, `stale`, `unmatched`, `ignored`, `wrong_store`,
//! `test_mode_refused`, `unknown_variant`, `superseded`), so Lemon Squeezy doesn't retry what
//! will never apply; only a database failure answers 500 (Lemon Squeezy retries it).
//!
//! **Which org.** Checkouts are made by this server only (`POST .../billing/checkout`), with
//! `custom_data` = the org id, the buyer's user id and an HMAC of both under a secret of this
//! server. The webhook proves Lemon Squeezy sent an event, not that the buyer may change the org it
//! names, so an event takes its org from the subscription id already on an `entitlements` row,
//! else from signed custom data; neither = `unmatched` (recorded, nothing changed;
//! `hitop-sync-server billing-events --unmatched` lists them).
//!
//! **Ordering.** Subscription events apply when the subscription's `updated_at` is not older than
//! the last one applied for it (`entitlements.ls_updated_at`, microseconds); older ones are
//! `stale`. Invoice events (`subscription_payment_*`) carry an invoice, not the subscription: they
//! apply only when the invoice is not older than that mark and never move it, except a refund,
//! which does (so the `subscription_updated` Lemon Squeezy sends with a renewal isn't held back
//! by the invoice before it). An event for another subscription than the org's: a live one takes
//! over (a new purchase), a lapsed one is `superseded` and ignored.
//!
//! **Status → limits** (`limits::Entitlement::plan_in_force`): `active`, `on_trial`, `past_due`
//! (the portal shows a "payment failed" banner while Lemon Squeezy retries) and `cancelled` (until
//! the period ends) keep the plan; `unpaid`, `expired`, `paused` and `refunded` are Free limits.
//! Nothing is ever deleted: creates beyond Free are refused, reads and pulls always work. A full
//! refund (`subscription_payment_refunded` of a fully refunded invoice, or `order_refunded` of the
//! subscription's order) sets `refunded`, and it stays: later subscription events update the ids
//! and period but not the status, until a newer paid invoice (`subscription_payment_success`) or
//! `subscription_resumed` lifts it. A partial refund changes nothing.
//!
//! **Seats.** Team is per seat: the subscription item's quantity is the seats bought. `POST
//! .../billing/seats` sets it through the API: more seats are invoiced at once (prorated), fewer
//! take effect at once with no credit (`disable_prorations`: the lower price starts at the next
//! renewal), and never below the seats in use. Pro is one person (its limits, 1 + 2 guests, don't
//! follow the quantity).
//!
//! **Trials.** One 14-day Team trial per team org, per account and per email domain (common free
//! mail domains excepted), no card. The trial ends at `period_end` (`expire`, at every limit
//! check and hourly): the org drops to Free limits with nothing deleted. Buying during a trial
//! replaces it.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::api::{bearer, body, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::{DbError, Tx};
use crate::limits;
use crate::orgs::{self, OrgRole};

pub const DEFAULT_API_URL: &str = "https://api.lemonsqueezy.com";
/// Seats of a Team trial (a proposal, like every number in limits.rs).
pub const TRIAL_SEATS: i64 = 5;
const MAX_SEATS: i64 = 1000;

/// One purchasable variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub plan: &'static str,
    pub period: &'static str,
    pub id: String,
}

#[derive(Clone)]
pub struct BillingConfig {
    pub webhook_secret: String,
    pub store_id: String,
    pub variants: Vec<Variant>,
    pub api_key: Option<String>,
    pub api_url: String,
    pub test_mode: bool,
}

impl std::fmt::Debug for BillingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secrets.
        f.debug_struct("BillingConfig").field("store_id", &self.store_id).field("variants", &self.variants).field("api_key", &self.api_key.is_some()).field("api_url", &self.api_url).field("test_mode", &self.test_mode).finish()
    }
}

fn secret_of(var: &dyn Fn(&str) -> Option<String>, name: &str) -> Result<Option<String>, String> {
    match (var(name), var(&format!("{name}_FILE"))) {
        (Some(s), _) => Ok(Some(s)),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(&path).map_err(|e| format!("{name}_FILE {path}: {e}"))?.trim().to_string()).filter(|s| !s.is_empty())),
        (None, None) => Ok(None),
    }
}

impl BillingConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<BillingConfig>, String> {
        let Some(webhook_secret) = secret_of(var, "HITOP_SYNC_LS_WEBHOOK_SECRET")? else {
            return Ok(None);
        };
        let store_id = var("HITOP_SYNC_LS_STORE_ID").ok_or("HITOP_SYNC_LS_STORE_ID must be set with HITOP_SYNC_LS_WEBHOOK_SECRET")?;
        let mut variants = Vec::new();
        for (plan, period) in [("pro", "annual"), ("pro", "monthly"), ("team", "annual"), ("team", "monthly")] {
            let name = format!("HITOP_SYNC_LS_VARIANT_{}_{}", plan.to_uppercase(), period.to_uppercase());
            if let Some(id) = var(&name) {
                if id.parse::<u64>().is_err() {
                    return Err(format!("{name} must be a Lemon Squeezy variant id (a number)"));
                }
                variants.push(Variant { plan, period, id });
            }
        }
        if variants.is_empty() {
            return Err("billing needs at least one HITOP_SYNC_LS_VARIANT_<PRO|TEAM>_<ANNUAL|MONTHLY>".into());
        }
        let api_url = var("HITOP_SYNC_LS_API_URL").unwrap_or_else(|| DEFAULT_API_URL.into()).trim_end_matches('/').to_string();
        if !crate::oidc::secure_url(&api_url) {
            return Err(format!("HITOP_SYNC_LS_API_URL must be an https:// URL (http:// only on localhost): {api_url}"));
        }
        let test_mode = match var("HITOP_SYNC_LS_TEST_MODE").as_deref() {
            None | Some("off") => false,
            Some("on") => true,
            Some(other) => return Err(format!("HITOP_SYNC_LS_TEST_MODE must be on or off, not {other}")),
        };
        Ok(Some(BillingConfig { webhook_secret, store_id, variants, api_key: secret_of(var, "HITOP_SYNC_LS_API_KEY")?, api_url, test_mode }))
    }

    fn variant(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|v| v.id == id)
    }
}

// --- signatures ----------------------------------------------------------------------------

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let pad = |b: u8| k.map(|x| x ^ b);
    let inner = Sha256::new().chain_update(pad(0x36)).chain_update(message).finalize();
    Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant time for equal lengths (the length of a hex digest is no secret).
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Lemon Squeezy's `X-Signature`: hex HMAC-SHA256 of the raw body.
pub fn signature_ok(secret: &str, body: &[u8], header: Option<&str>) -> bool {
    let Some(h) = header else { return false };
    let expected = hex(&hmac_sha256(secret.as_bytes(), body));
    same(expected.as_bytes(), h.trim().to_ascii_lowercase().as_bytes())
}

fn custom_sig(tx: &mut Tx, org: &str, user: &str) -> Result<String, ApiError> {
    let key = crate::password::server_secret(tx, "billing-custom-data")?;
    Ok(hex(&hmac_sha256(&key, format!("hitop-billing/v1\n{org}\n{user}").as_bytes())))
}

// --- time ----------------------------------------------------------------------------------

/// `2026-11-03T10:00:00.000000Z` (or without fraction, or with an offset of `+00:00`) →
/// microseconds since 1970. `None` for anything else.
pub fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim();
    let (main, zone_ok) = if let Some(m) = s.strip_suffix('Z') { (m, true) } else if let Some(m) = s.strip_suffix("+00:00") { (m, true) } else { (s, false) };
    if !zone_ok || main.len() < 19 {
        return None;
    }
    let b = main.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || !(b[10] == b'T' || b[10] == b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| main.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0..4)?, n(5..7)?, n(8..10)?, n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let mut micros = 0i64;
    if let Some(frac) = main.get(19..).filter(|f| !f.is_empty()) {
        let digits = frac.strip_prefix('.')?;
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let six: String = digits.chars().chain(std::iter::repeat('0')).take(6).collect();
        micros = six.parse().ok()?;
    }
    // Days from the civil date (Howard Hinnant's algorithm).
    let (yy, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + se) * 1_000_000) + micros)
}

fn secs(attrs: &Json, field: &str) -> Option<i64> {
    attrs[field].as_str().and_then(parse_time).map(|us| us.div_euclid(1_000_000))
}

/// A JSON id (Lemon Squeezy sends numbers in attributes and strings in `data.id`).
fn id_of(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

// --- the entitlements row --------------------------------------------------------------------

/// Statuses that keep a paid plan's limits; anything else is Free (limits.rs).
pub const LIVE: &[&str] = &["active", "on_trial", "past_due", "cancelled", "trialing"];

#[derive(Debug, Default)]
struct Row {
    plan: String,
    seats: i64,
    status: String,
    period_end: i64,
    sub: String,
    item: String,
    order: String,
    mark: i64,
}

fn row(tx: &mut Tx, org: &str) -> Result<Row, DbError> {
    Ok(tx
        .query_one("SELECT plan, seats, status, period_end, ls_subscription_id, ls_item_id, ls_order_id, ls_updated_at FROM entitlements WHERE org_id = $1", &[org.into()])?
        .map(|r| Row { plan: r.text(0), seats: r.int(1), status: r.text(2), period_end: r.int(3), sub: r.text(4), item: r.text(5), order: r.text(6), mark: r.int(7) })
        .unwrap_or_else(|| Row { plan: "free".into(), status: "active".into(), ..Row::default() }))
}

fn ensure_row(state: &State, tx: &mut Tx, org: &str) -> Result<(), DbError> {
    tx.execute(
        "INSERT INTO entitlements (org_id, plan, seats, limits, status, period_end, updated_at) VALUES ($1, 'free', 0, '{}', 'active', 0, $2) ON CONFLICT (org_id) DO NOTHING",
        &[org.into(), state.now().into()],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn record_change(state: &State, tx: &mut Tx, org: &str, before: &Row, after: &Row, by: &str, event: &str, actor: Option<&User>) -> Result<(), DbError> {
    if (before.plan.as_str(), before.seats, before.status.as_str()) == (after.plan.as_str(), after.seats, after.status.as_str()) {
        return Ok(());
    }
    tx.execute("UPDATE orgs SET plan = $1 WHERE id = $2", &[(&after.plan).into(), org.into()])?;
    let chain = orgs::chain(org);
    audit::record(
        tx,
        state,
        Event::new(&chain, "plan_changed", actor).target(org).detail(json!({
            "from": before.plan, "to": after.plan, "fromStatus": before.status, "status": after.status, "seats": after.seats, "by": by, "event": event,
        })),
    )
}

fn write_row(state: &State, tx: &mut Tx, org: &str, r: &Row, customer: Option<&str>, variant: Option<&str>) -> Result<(), DbError> {
    ensure_row(state, tx, org)?;
    tx.execute(
        "UPDATE entitlements SET plan = $1, seats = $2, status = $3, period_end = $4, ls_subscription_id = $5, ls_item_id = $6, ls_order_id = $7, ls_updated_at = $8, updated_at = $9 WHERE org_id = $10",
        &[(&r.plan).into(), r.seats.into(), (&r.status).into(), r.period_end.into(), (&r.sub).into(), (&r.item).into(), (&r.order).into(), r.mark.into(), state.now().into(), org.into()],
    )?;
    if let Some(c) = customer.filter(|c| !c.is_empty()) {
        tx.execute("UPDATE entitlements SET ls_customer_id = $1 WHERE org_id = $2", &[c.into(), org.into()])?;
    }
    if let Some(v) = variant.filter(|v| !v.is_empty()) {
        tx.execute("UPDATE entitlements SET ls_variant_id = $1 WHERE org_id = $2", &[v.into(), org.into()])?;
    }
    Ok(())
}

/// Ends trials and cancelled subscriptions whose period is over: Free limits, nothing deleted.
/// `org` = just that one (limit checks), `None` = all (hourly, `over-limits`).
pub fn expire(state: &State, tx: &mut Tx, org: Option<&str>) -> Result<u64, DbError> {
    let now = state.now();
    let due = match org {
        Some(o) => tx.query("SELECT org_id, status FROM entitlements WHERE org_id = $1 AND status IN ('trialing', 'cancelled') AND period_end > 0 AND period_end <= $2", &[o.into(), now.into()])?,
        None => tx.query("SELECT org_id, status FROM entitlements WHERE status IN ('trialing', 'cancelled') AND period_end > 0 AND period_end <= $1", &[now.into()])?,
    };
    for r in &due {
        let o = r.text(0);
        let before = row(tx, &o)?;
        tx.execute("UPDATE entitlements SET status = 'expired', updated_at = $1 WHERE org_id = $2", &[now.into(), (&o).into()])?;
        let after = Row { status: "expired".into(), ..row(tx, &o)? };
        let by = if r.text(1) == "trialing" { "trial_end" } else { "period_end" };
        record_change(state, tx, &o, &before, &after, by, by, None)?;
    }
    Ok(due.len() as u64)
}

// --- the webhook ---------------------------------------------------------------------------

fn config(state: &State) -> Result<&BillingConfig, ApiError> {
    state.config.billing.as_ref().ok_or_else(|| ApiError::new(404, "billing_off", "this server sells no plans (billing isn't configured)"))
}

/// What one delivery did.
struct Outcome {
    outcome: &'static str,
    org: String,
    subscription: String,
    updated_at: i64,
}

impl Outcome {
    fn new(outcome: &'static str) -> Outcome {
        Outcome { outcome, org: String::new(), subscription: String::new(), updated_at: 0 }
    }
}

/// `POST /v1/billing/lemonsqueezy`: a Lemon Squeezy webhook delivery (module docs).
pub fn webhook(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let c = config(state)?;
    let sig = request.header("x-signature").map(|h| h.value.to_string());
    if !signature_ok(&c.webhook_secret, &request.body, sig.as_deref()) {
        return Err(ApiError::new(401, "bad_signature", "X-Signature doesn't match the body"));
    }
    let doc: Json = serde_json::from_slice(&request.body).map_err(|_| ApiError::bad("the body isn't JSON"))?;
    let event = doc["meta"]["event_name"].as_str().unwrap_or_default().to_string();
    if let Some(h) = request.header("x-event-name") {
        if h.value != event.as_str() {
            return Err(ApiError::bad("X-Event-Name doesn't match meta.event_name"));
        }
    }
    let id = hex(&Sha256::digest(&request.body));
    state.db.tx(|tx| {
        if tx.query_one("SELECT outcome FROM billing_events WHERE id = $1", &[(&id).into()])?.is_some() {
            return Ok((200, json!({ "outcome": "duplicate" })));
        }
        let test = doc["data"]["attributes"]["test_mode"].as_bool().unwrap_or(false);
        let o = apply(state, tx, c, &event, &doc)?;
        tx.execute(
            "INSERT INTO billing_events (id, event_name, org_id, subscription_id, resource_updated_at, test_mode, outcome, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[(&id).into(), (&event).into(), (&o.org).into(), (&o.subscription).into(), o.updated_at.into(), i64::from(test).into(), o.outcome.into(), state.now().into()],
        )?;
        if o.outcome != "applied" {
            eprintln!("hitop-sync-server: billing: {event} {}: {}", if o.subscription.is_empty() { "-" } else { &o.subscription }, o.outcome);
        }
        Ok((200, json!({ "outcome": o.outcome, "org": if o.org.is_empty() { Json::Null } else { o.org.clone().into() } })))
    })
}

/// The org an event belongs to: the subscription's row, else verified custom data.
fn org_for(tx: &mut Tx, subscription: &str, custom: &Json) -> Result<Option<String>, ApiError> {
    if !subscription.is_empty() {
        if let Some(r) = tx.query_one("SELECT org_id FROM entitlements WHERE ls_subscription_id = $1", &[subscription.into()])? {
            return Ok(Some(r.text(0)));
        }
    }
    let (org, user, sig) = (id_of(&custom["org_id"]), id_of(&custom["user_id"]), custom["sig"].as_str().unwrap_or_default().to_string());
    if org.is_empty() || user.is_empty() || sig.is_empty() {
        return Ok(None);
    }
    if !same(custom_sig(tx, &org, &user)?.as_bytes(), sig.as_bytes()) {
        return Ok(None);
    }
    Ok(tx.query_one("SELECT id FROM orgs WHERE id = $1", &[(&org).into()])?.map(|r| r.text(0)))
}

fn apply(state: &State, tx: &mut Tx, c: &BillingConfig, event: &str, doc: &Json) -> Result<Outcome, ApiError> {
    let data = &doc["data"];
    let attrs = &data["attributes"];
    let custom = &doc["meta"]["custom_data"];
    if !attrs["store_id"].is_null() && id_of(&attrs["store_id"]) != c.store_id {
        return Ok(Outcome::new("wrong_store"));
    }
    if attrs["test_mode"].as_bool() == Some(true) && !c.test_mode {
        return Ok(Outcome::new("test_mode_refused"));
    }
    let updated = attrs["updated_at"].as_str().and_then(parse_time).unwrap_or(0);
    match data["type"].as_str().unwrap_or_default() {
        "subscriptions" => subscription_event(state, tx, c, event, &id_of(&data["id"]), attrs, custom, updated),
        "subscription-invoices" => invoice_event(state, tx, event, &id_of(&attrs["subscription_id"]), attrs, custom, updated),
        "orders" => order_event(state, tx, event, &id_of(&data["id"]), attrs, custom, updated),
        _ => Ok(Outcome::new("ignored")),
    }
}

#[allow(clippy::too_many_arguments)]
fn subscription_event(state: &State, tx: &mut Tx, c: &BillingConfig, event: &str, sub: &str, attrs: &Json, custom: &Json, updated: i64) -> Result<Outcome, ApiError> {
    let mut out = Outcome { outcome: "applied", org: String::new(), subscription: sub.to_string(), updated_at: updated };
    let Some(org) = org_for(tx, sub, custom)? else {
        out.outcome = "unmatched";
        return Ok(out);
    };
    out.org = org.clone();
    let variant_id = id_of(&attrs["variant_id"]);
    let Some(variant) = c.variant(&variant_id) else {
        out.outcome = "unknown_variant";
        return Ok(out);
    };
    let status = attrs["status"].as_str().unwrap_or_default().to_string();
    let before = row(tx, &org)?;
    if !before.sub.is_empty() && before.sub != sub {
        // Another subscription: a live one replaces the org's (a new purchase), a lapsed one
        // never overrides it.
        if !LIVE.contains(&status.as_str()) {
            out.outcome = "superseded";
            return Ok(out);
        }
    } else if updated < before.mark {
        out.outcome = "stale";
        return Ok(out);
    }
    let same_sub = before.sub == sub;
    let mut after = Row {
        plan: variant.plan.to_string(),
        seats: if variant.plan == "team" { attrs["first_subscription_item"]["quantity"].as_i64().unwrap_or(before.seats).clamp(1, MAX_SEATS) } else { 1 },
        status: status.clone(),
        period_end: secs(attrs, "ends_at").or_else(|| secs(attrs, "renews_at")).unwrap_or(before.period_end),
        sub: sub.to_string(),
        item: Some(id_of(&attrs["first_subscription_item"]["id"])).filter(|s| !s.is_empty()).unwrap_or_else(|| if same_sub { before.item.clone() } else { String::new() }),
        order: Some(id_of(&attrs["order_id"])).filter(|s| !s.is_empty()).unwrap_or_else(|| if same_sub { before.order.clone() } else { String::new() }),
        mark: updated.max(if same_sub { before.mark } else { 0 }),
    };
    // A refund stays until a resume or a newer paid invoice (module docs).
    if same_sub && before.status == "refunded" && event != "subscription_resumed" {
        after.status = "refunded".into();
    }
    write_row(state, tx, &org, &after, Some(&id_of(&attrs["customer_id"])), Some(&variant_id))?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn invoice_event(state: &State, tx: &mut Tx, event: &str, sub: &str, attrs: &Json, custom: &Json, updated: i64) -> Result<Outcome, ApiError> {
    let mut out = Outcome { outcome: "applied", org: String::new(), subscription: sub.to_string(), updated_at: updated };
    let Some(org) = org_for(tx, sub, custom)? else {
        out.outcome = "unmatched";
        return Ok(out);
    };
    out.org = org.clone();
    let before = row(tx, &org)?;
    if before.sub != sub {
        out.outcome = "superseded";
        return Ok(out);
    }
    if updated < before.mark {
        out.outcome = "stale";
        return Ok(out);
    }
    let mut after = row(tx, &org)?;
    match event {
        "subscription_payment_failed" if matches!(before.status.as_str(), "active" | "on_trial") => after.status = "past_due".into(),
        "subscription_payment_success" | "subscription_payment_recovered" if matches!(before.status.as_str(), "past_due" | "unpaid" | "refunded") => after.status = "active".into(),
        "subscription_payment_refunded" if attrs["refunded"].as_bool() == Some(true) => {
            after.status = "refunded".into();
            after.mark = before.mark.max(updated);
        }
        _ => {
            out.outcome = "ignored";
            return Ok(out);
        }
    }
    write_row(state, tx, &org, &after, None, None)?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    Ok(out)
}

fn order_event(state: &State, tx: &mut Tx, event: &str, order: &str, attrs: &Json, custom: &Json, updated: i64) -> Result<Outcome, ApiError> {
    let mut out = Outcome { outcome: "ignored", org: String::new(), subscription: String::new(), updated_at: updated };
    if event != "order_refunded" {
        // order_created: the subscription events carry the state.
        return Ok(out);
    }
    let found = tx.query_one("SELECT org_id, ls_subscription_id FROM entitlements WHERE ls_order_id = $1 AND ls_order_id <> ''", &[order.into()])?;
    let Some(r) = found else {
        out.outcome = if org_for(tx, "", custom)?.is_some() { "ignored" } else { "unmatched" };
        return Ok(out);
    };
    let org = r.text(0);
    out.org = org.clone();
    out.subscription = r.text(1);
    if attrs["refunded"].as_bool() != Some(true) && attrs["status"].as_str() != Some("refunded") {
        // A partial refund changes nothing.
        return Ok(out);
    }
    let before = row(tx, &org)?;
    let after = Row { status: "refunded".into(), mark: before.mark.max(updated), ..row(tx, &org)? };
    write_row(state, tx, &org, &after, None, None)?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    out.outcome = "applied";
    Ok(out)
}

/// Event names as `&'static str` for the audit detail (unknown ones as "other").
fn event_name(e: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "subscription_created",
        "subscription_updated",
        "subscription_cancelled",
        "subscription_resumed",
        "subscription_expired",
        "subscription_paused",
        "subscription_unpaused",
        "subscription_payment_success",
        "subscription_payment_failed",
        "subscription_payment_recovered",
        "subscription_payment_refunded",
        "order_created",
        "order_refunded",
    ];
    KNOWN.iter().find(|k| **k == e).copied().unwrap_or("other")
}

// --- the portal's endpoints ------------------------------------------------------------------

/// The signed-in person with a session (not an access token: these move money) and their role in
/// `org` (404 when not a member).
fn caller(state: &State, tx: &mut Tx, request: &Request, org: &str, need: OrgRole) -> Result<(User, orgs::Org), ApiError> {
    let user = authenticate(state, tx, request)?;
    user.person()?;
    orgs::ensure_personal(state, tx, &user)?;
    let o = orgs::org_membership(tx, org, &user)?;
    if need > OrgRole::Member && bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden("billing needs a signed-in session, not an access token"));
    }
    if o.role < need {
        return Err(ApiError::forbidden(format!("this needs the {} role in the organization", need.as_str())));
    }
    Ok((user, o))
}

fn offers(c: Option<&BillingConfig>, personal: bool) -> Vec<Json> {
    let plan = if personal { "pro" } else { "team" };
    c.map(|c| c.variants.iter().filter(|v| v.plan == plan).map(|v| json!({ "plan": v.plan, "period": v.period })).collect()).unwrap_or_default()
}

/// A Lemon Squeezy subscription that is still running (perhaps unpaid or paused): change it in
/// the customer portal rather than buying a second one.
fn subscribed(r: &Row) -> bool {
    !r.sub.is_empty() && matches!(r.status.as_str(), "active" | "on_trial" | "past_due" | "cancelled" | "paused" | "unpaid")
}

fn trial_domain(email: &str) -> String {
    const FREE_MAIL: &[&str] = &[
        "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com", "yahoo.com", "icloud.com", "me.com", "proton.me", "protonmail.com", "gmx.de", "gmx.net", "web.de", "mail.ru", "yandex.ru", "aol.com", "fastmail.com", "zoho.com", "example.com",
    ];
    let d = email.rsplit_once('@').map(|(_, d)| d.to_lowercase()).unwrap_or_default();
    if FREE_MAIL.contains(&d.as_str()) {
        String::new()
    } else {
        d
    }
}

/// Whether `user` may start a trial for `org` (None) or why not.
fn trial_refusal(state: &State, tx: &mut Tx, org: &orgs::Org, user: &User, r: &Row) -> Result<Option<&'static str>, DbError> {
    if !state.config.plan_limits || state.config.trial_days <= 0 {
        return Ok(Some("trials are off on this server"));
    }
    if org.personal {
        return Ok(Some("the Team trial is for an organization; create one first"));
    }
    if tx.query_one("SELECT org_id FROM trials WHERE org_id = $1", &[(&org.id).into()])?.is_some() {
        return Ok(Some("this organization already had its trial"));
    }
    if tx.query_one("SELECT org_id FROM trials WHERE user_id = $1", &[(&user.id).into()])?.is_some() {
        return Ok(Some("your account already started a trial"));
    }
    let domain = trial_domain(&user.email);
    if !domain.is_empty() && tx.query_one("SELECT org_id FROM trials WHERE email_domain = $1", &[(&domain).into()])?.is_some() {
        return Ok(Some("an account of your email domain already started a trial"));
    }
    if subscribed(r) || r.plan != "free" {
        return Ok(Some("this organization already has a plan"));
    }
    Ok(None)
}

/// `GET /v1/orgs/:org/billing` (member): plan, status, seats bought and used, what can be bought.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let (user, o) = caller(state, tx, request, org, OrgRole::Member)?;
        expire(state, tx, Some(org))?;
        let r = row(tx, org)?;
        let e = limits::entitlement(tx, org)?;
        let used = limits::usage(state, tx, org)?.seats;
        let c = state.config.billing.as_ref();
        let trial_no = trial_refusal(state, tx, &o, &user, &r)?;
        let trial = tx.query_one("SELECT started_at, ends_at FROM trials WHERE org_id = $1", &[org.into()])?;
        Ok((
            200,
            json!({
                "org": org,
                "enabled": c.is_some(),
                "checkout": c.is_some_and(|c| c.api_key.is_some()),
                "testMode": c.is_some_and(|c| c.test_mode),
                "plan": r.plan,
                "planInForce": e.plan_in_force(),
                "status": r.status,
                "seats": r.seats,
                "seatsUsed": used,
                "periodEnd": r.period_end,
                "pastDue": r.status == "past_due",
                "subscribed": subscribed(&r),
                "offers": offers(c, o.personal),
                "trial": {
                    "available": trial_no.is_none(),
                    "why": trial_no,
                    "days": state.config.trial_days,
                    "seats": TRIAL_SEATS,
                    "startedAt": trial.as_ref().map(|t| t.int(0)),
                    "endsAt": trial.as_ref().map(|t| t.int(1)),
                },
            }),
        ))
    })
}

/// `POST /v1/orgs/:org/billing/trial` (owner): 14 days of Team, no card.
pub fn start_trial(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let (user, o) = caller(state, tx, request, org, OrgRole::Owner)?;
        let before = row(tx, org)?;
        if let Some(why) = trial_refusal(state, tx, &o, &user, &before)? {
            return Err(ApiError::conflict("trial_refused", why, json!({})));
        }
        let now = state.now();
        let ends = now + state.config.trial_days * 86_400;
        tx.execute(
            "INSERT INTO trials (org_id, user_id, email_domain, started_at, ends_at) VALUES ($1, $2, $3, $4, $5)",
            &[org.into(), (&user.id).into(), trial_domain(&user.email).into(), now.into(), ends.into()],
        )?;
        let after = Row { plan: "team".into(), seats: TRIAL_SEATS, status: "trialing".into(), period_end: ends, ..row(tx, org)? };
        write_row(state, tx, org, &after, None, None)?;
        record_change(state, tx, org, &before, &after, "trial", "trial_started", Some(&user))?;
        Ok((201, json!({ "org": org, "plan": "team", "status": "trialing", "seats": TRIAL_SEATS, "endsAt": ends })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckoutIn {
    plan: String,
    #[serde(default)]
    period: Option<String>,
    #[serde(default)]
    seats: Option<i64>,
}

fn api_key(c: &BillingConfig) -> Result<&str, ApiError> {
    c.api_key.as_deref().ok_or_else(|| ApiError::conflict("billing_api_off", "buying needs the Lemon Squeezy API key on this server (HITOP_SYNC_LS_API_KEY)", json!({})))
}

/// One Lemon Squeezy API call (JSON:API), outside any database transaction.
fn ls_call(state: &State, c: &BillingConfig, method: &str, path: &str, body: Option<&Json>) -> Result<Json, ApiError> {
    let key = api_key(c)?;
    let url = format!("{}{path}", c.api_url);
    let http = &state.billing_http;
    let auth = format!("Bearer {key}");
    let result = match (method, body) {
        ("POST", Some(b)) => http.post(&url).header("Accept", "application/vnd.api+json").header("Content-Type", "application/vnd.api+json").header("Authorization", &auth).send(b.to_string()),
        ("PATCH", Some(b)) => http.patch(&url).header("Accept", "application/vnd.api+json").header("Content-Type", "application/vnd.api+json").header("Authorization", &auth).send(b.to_string()),
        _ => http.get(&url).header("Accept", "application/vnd.api+json").header("Authorization", &auth).call(),
    };
    let unreachable = |e: String| {
        eprintln!("hitop-sync-server: Lemon Squeezy: {e}");
        ApiError::new(503, "billing_unreachable", "Lemon Squeezy is unreachable; try again")
    };
    let mut res = result.map_err(|e| unreachable(format!("{method} {path}: {e}")))?;
    let status = res.status().as_u16();
    let text = res.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| unreachable(format!("{method} {path}: {e}")))?;
    let doc: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
    if !(200..300).contains(&status) {
        let detail = doc["errors"][0]["detail"].as_str().unwrap_or("").to_string();
        eprintln!("hitop-sync-server: Lemon Squeezy {method} {path}: {status} {detail}");
        return Err(ApiError::new(502, "billing_refused", format!("Lemon Squeezy refused the request ({status}){}", if detail.is_empty() { String::new() } else { format!(": {detail}") })));
    }
    Ok(doc)
}

/// `POST /v1/orgs/:org/billing/checkout {"plan","period","seats"}` (admin) → `{url}`: a Lemon
/// Squeezy checkout for the org (Pro for a personal org, Team per seat for a team org).
pub fn checkout(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let input: CheckoutIn = body(request)?;
    let period = input.period.unwrap_or_else(|| "annual".into());
    let (user, body) = state.db.tx(|tx| -> Result<(User, Json), ApiError> {
        let (user, o) = caller(state, tx, request, &org, OrgRole::Admin)?;
        let want = if o.personal { "pro" } else { "team" };
        if input.plan != want {
            return Err(ApiError::bad(if o.personal { "a personal account buys Pro; for Team, create an organization" } else { "an organization buys Team (per seat); Pro is for one person's personal account" }));
        }
        let variant = c.variants.iter().find(|v| v.plan == input.plan && v.period == period).ok_or_else(|| ApiError::bad(format!("{} {period} isn't on sale here", input.plan)))?;
        let r = row(tx, &org)?;
        if subscribed(&r) {
            return Err(ApiError::conflict("already_subscribed", "this organization has a subscription: change it in the billing portal", json!({})));
        }
        let used = limits::usage(state, tx, &org)?.seats.max(1);
        let quantity = if want == "team" { input.seats.unwrap_or(used) } else { 1 };
        if !(1..=MAX_SEATS).contains(&quantity) {
            return Err(ApiError::bad(format!("seats must be 1 to {MAX_SEATS}")));
        }
        if quantity < used {
            return Err(ApiError::bad(format!("the organization uses {used} seat(s): buy at least that many")));
        }
        let sig = custom_sig(tx, &org, &user.id)?;
        let body = json!({
            "data": {
                "type": "checkouts",
                "attributes": {
                    "checkout_data": {
                        "email": user.email,
                        "custom": { "org_id": org, "user_id": user.id, "sig": sig },
                        "variant_quantities": [{ "variant_id": variant.id.parse::<u64>().unwrap_or(0), "quantity": quantity }],
                    },
                    "product_options": {
                        "enabled_variants": [variant.id.parse::<u64>().unwrap_or(0)],
                        "redirect_url": format!("{}#/orgs/{org}/plan", state.config.portal_url),
                    },
                    "test_mode": c.test_mode,
                },
                "relationships": {
                    "store": { "data": { "type": "stores", "id": c.store_id } },
                    "variant": { "data": { "type": "variants", "id": variant.id } },
                },
            }
        });
        Ok((user, body))
    })?;
    let doc = ls_call(state, c, "POST", "/v1/checkouts", Some(&body))?;
    let url = doc["data"]["attributes"]["url"].as_str().ok_or_else(|| ApiError::new(502, "billing_refused", "Lemon Squeezy sent no checkout URL"))?.to_string();
    state.db.tx(|tx| audit::record(tx, state, Event::new(&orgs::chain(&org), "checkout_started", Some(&user)).target(&org).detail(json!({ "plan": input.plan, "period": period }))))?;
    Ok((200, json!({ "url": url })))
}

/// `GET /v1/orgs/:org/billing/portal` (admin) → `{url}`: the Lemon Squeezy customer portal of the
/// org's subscription (card, receipts, cancel, monthly/annual), a fresh signed link each time.
pub fn portal(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let sub = state.db.tx(|tx| -> Result<String, ApiError> {
        caller(state, tx, request, &org, OrgRole::Admin)?;
        let r = row(tx, &org)?;
        if r.sub.is_empty() {
            return Err(ApiError::conflict("no_subscription", "this organization has no subscription yet", json!({})));
        }
        Ok(r.sub)
    })?;
    let doc = ls_call(state, c, "GET", &format!("/v1/subscriptions/{sub}"), None)?;
    let url = doc["data"]["attributes"]["urls"]["customer_portal"].as_str().ok_or_else(|| ApiError::new(502, "billing_refused", "Lemon Squeezy sent no customer portal URL"))?;
    Ok((200, json!({ "url": url })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeatsIn {
    seats: i64,
}

/// `POST /v1/orgs/:org/billing/seats {"seats"}` (admin): Team seats bought (module docs).
pub fn seats(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    api_key(c)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let input: SeatsIn = body(request)?;
    let (user, item, before) = state.db.tx(|tx| -> Result<(User, String, i64), ApiError> {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Admin)?;
        let r = row(tx, &org)?;
        if r.plan != "team" || r.item.is_empty() || !matches!(r.status.as_str(), "active" | "on_trial" | "past_due") {
            return Err(ApiError::conflict("no_subscription", "seats change on a live Team subscription", json!({})));
        }
        let used = limits::usage(state, tx, &org)?.seats;
        if !(1..=MAX_SEATS).contains(&input.seats) {
            return Err(ApiError::bad(format!("seats must be 1 to {MAX_SEATS}")));
        }
        if input.seats < used {
            return Err(ApiError::bad(format!("the organization uses {used} seat(s): remove people first")));
        }
        Ok((user, r.item, r.seats))
    })?;
    if input.seats == before {
        return Ok((200, json!({ "org": org, "seats": before })));
    }
    let more = input.seats > before;
    let body = json!({
        "data": {
            "type": "subscription-items",
            "id": item,
            "attributes": if more { json!({ "quantity": input.seats, "invoice_immediately": true }) } else { json!({ "quantity": input.seats, "disable_prorations": true }) },
        }
    });
    let doc = ls_call(state, c, "PATCH", &format!("/v1/subscription-items/{item}"), Some(&body))?;
    let quantity = doc["data"]["attributes"]["quantity"].as_i64().unwrap_or(input.seats).clamp(1, MAX_SEATS);
    let item_updated = doc["data"]["attributes"]["updated_at"].as_str().and_then(parse_time).unwrap_or(0);
    state.db.tx(|tx| -> Result<(), ApiError> {
        let b = row(tx, &org)?;
        if b.item != item {
            return Ok(());
        }
        let after = Row { seats: quantity, mark: b.mark.max(item_updated), ..row(tx, &org)? };
        write_row(state, tx, &org, &after, None, None)?;
        record_change(state, tx, &org, &b, &after, "seats", "seats_changed", Some(&user))?;
        Ok(())
    })?;
    Ok((200, json!({ "org": org, "seats": quantity })))
}

/// `hitop-sync-server billing-events [--unmatched]`: the latest deliveries (ids and outcomes).
pub fn list_events(state: &State, unmatched: bool) -> Result<Vec<Json>, DbError> {
    state.db.read_tx(|tx| {
        let rows = tx.query(
            "SELECT id, event_name, org_id, subscription_id, outcome, test_mode, received_at FROM billing_events WHERE ($1 = 0 OR outcome = 'unmatched') ORDER BY received_at DESC, id LIMIT 200",
            &[i64::from(unmatched).into()],
        )?;
        Ok(rows
            .iter()
            .map(|r| json!({ "id": r.text(0), "event": r.text(1), "org": r.text(2), "subscription": r.text(3), "outcome": r.text(4), "testMode": r.int(5) != 0, "receivedAt": r.int(6) }))
            .collect())
    })
}

/// `hitop-sync-server billing-link <org> <subscription id>`: ties a subscription bought outside
/// the portal's checkout (an `unmatched` event) to an org; the next event for it applies.
pub fn link(state: &State, org: &str, subscription: &str) -> Result<(), String> {
    state
        .db
        .tx(|tx| -> Result<(), DbError> {
            if tx.query_one("SELECT id FROM orgs WHERE id = $1", &[org.into()])?.is_none() {
                return Err(DbError(format!("no org {org}")));
            }
            if let Some(r) = tx.query_one("SELECT org_id FROM entitlements WHERE ls_subscription_id = $1", &[subscription.into()])? {
                return Err(DbError(format!("subscription {subscription} already belongs to {}", r.text(0))));
            }
            ensure_row(state, tx, org)?;
            tx.execute("UPDATE entitlements SET ls_subscription_id = $1, ls_updated_at = 0 WHERE org_id = $2", &[subscription.into(), org.into()])?;
            audit::record(tx, state, Event::new(&orgs::chain(org), "billing_linked", None).target(org).detail(json!({ "subscription": subscription, "by": "operator" })))
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231() {
        // Test cases 1, 2 and 6 (a key longer than the block).
        assert_eq!(hex(&hmac_sha256(&[0x0b; 20], b"Hi There")), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        assert_eq!(hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")), "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54");
    }

    #[test]
    fn signatures() {
        let body = br#"{"meta":{"event_name":"order_created"}}"#;
        let good = hex(&hmac_sha256(b"s3cret", body));
        assert!(signature_ok("s3cret", body, Some(&good)));
        assert!(signature_ok("s3cret", body, Some(&good.to_uppercase())));
        assert!(!signature_ok("s3cret", body, None));
        assert!(!signature_ok("other", body, Some(&good)));
        assert!(!signature_ok("s3cret", b"{}", Some(&good)));
        assert!(!signature_ok("s3cret", body, Some(&good[..63])));
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("1970-01-01T00:00:01.000001Z"), Some(1_000_001));
        assert_eq!(parse_time("2022-11-12T00:00:00.000000Z"), Some(1_668_211_200_000_000));
        assert_eq!(parse_time("2024-02-29T12:30:15.5Z"), Some(1_709_209_815_500_000));
        assert_eq!(parse_time("2026-11-03T10:00:00+00:00"), parse_time("2026-11-03T10:00:00Z"));
        for bad in ["", "2026-11-03", "2026-13-01T00:00:00Z", "2026-11-03T10:00:00+02:00", "2026-11-03T10:00:00.Z", "x026-11-03T10:00:00Z"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
    }

    #[test]
    fn trial_domains_skip_free_mail() {
        assert_eq!(trial_domain("a@Gmail.com"), "");
        assert_eq!(trial_domain("a@acme.io"), "acme.io");
    }
}
