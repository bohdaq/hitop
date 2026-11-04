//! Billing for HITOP Cloud (spec/CLOUD_SERVICE.md section 6, tranche C3): **Stripe Managed
//! Payments** (Stripe is the merchant of record: it sells to the buyer and handles VAT and sales
//! tax), a 14-day Team trial run by this server, and the endpoints the portal uses. Off unless
//! configured (a self-hosted server sells nothing): every endpoint here answers 404 `billing_off`
//! without the Stripe keys.
//!
//! Settings:
//!
//! - `HITOP_SYNC_STRIPE_SECRET_KEY` (or `_FILE`): a secret key (`sk_…`) or restricted key
//!   (`rk_…`; write on Checkout Sessions, Subscriptions and Customer portal, read on Prices). Its mode decides
//!   which events count: a test key (`_test_`) only test-mode events, a live key only live ones.
//! - `HITOP_SYNC_STRIPE_WEBHOOK_SECRET` (or `_FILE`): the webhook endpoint's signing secret
//!   (`whsec_…`). Billing is on when both are set (one without the other is a startup error).
//! - Prices: by default found by their **lookup keys** in Stripe (`pro_monthly`, `pro_yearly`,
//!   `team_monthly`, `team_yearly`, `business_monthly`, `business_yearly`: `GET /v1/prices`,
//!   cached 10 minutes), so the two secrets are all the server needs. Optional overrides:
//!   `HITOP_SYNC_STRIPE_PRICE_<PRO|TEAM|BUSINESS>_<MONTHLY|ANNUAL>` = a price id (`price_…`). The
//!   price decides the plan (by id, else by the lookup key the event's price carries); amounts
//!   live in Stripe. A plan isn't sold until a price for it is found.
//! - `HITOP_SYNC_STRIPE_API_URL`: the API (default `https://api.stripe.com`; tests use a fake).
//! - `HITOP_SYNC_TRIAL_DAYS`: the Team trial (default 14; 0 = no trials). Trials run on this
//!   server, no card, but only where plans are sold: `HITOP_SYNC_LIMITS=plans` and billing on.
//!
//! Every API call sends `Stripe-Version: 2026-10-28.endive` (`STRIPE_VERSION`); the webhook
//! endpoint must be created with the same version, since event bodies follow the endpoint's.
//!
//! **Webhook** (`POST /v1/billing/stripe`). `Stripe-Signature: t=<secs>,v1=<hex>[,v1=…]` must hold
//! a `v1` that is the hex HMAC-SHA256 of `"<t>.<raw body>"` under the signing secret (constant-time
//! compare; two `v1` while a secret is rolled) and `t` within 5 minutes of now, before the body is
//! parsed: anything else is a 401 and nothing is read. Deliveries are idempotent by the event id
//! (`evt_…`): one already seen answers 200 `duplicate` and changes nothing. Bodies are never stored
//! (they hold the buyer's name, address and email); `billing_events` keeps the event id, type,
//! org, subscription id, `created` and the outcome. An event of the other mode than the key
//! (`livemode`) is `livemode_mismatch`: a test purchase never grants a live plan. Every verified
//! delivery answers 200 with its outcome (`applied`, `duplicate`, `stale`, `unmatched`, `ignored`,
//! `livemode_mismatch`, `unknown_price`, `superseded`), so Stripe doesn't retry what will never
//! apply; only a database failure answers 500 (Stripe retries it).
//!
//! **Events.** `checkout.session.completed` ties the session's subscription and customer to the
//! org; `customer.subscription.*` (`created`, `updated`, `deleted`, `paused`, `resumed`) set plan
//! (from the price), seats (the item quantity), status and period end; `invoice.payment_failed`
//! → `past_due` (the portal's banner, plan kept), `invoice.paid` → `active` again; a fully
//! refunded `charge.refunded` → `refunded`. Anything else is `ignored`.
//!
//! **Columns.** Migration 7 named them for the first provider; they hold Stripe's ids now:
//! `entitlements.ls_customer_id` = `cus_…`, `ls_subscription_id` = `sub_…`, `ls_variant_id` = the
//! price `price_…`, `ls_item_id` = the subscription item `si_…`, `ls_order_id` = the Checkout
//! Session `cs_…`, `ls_updated_at` = the ordering mark (event `created`, microseconds);
//! `billing_events.id` = the event id, `test_mode` = not `livemode`.
//!
//! **Which org.** Checkouts are made by this server only (`POST .../billing/checkout`), with
//! `client_reference_id` = the org id and metadata (on the session and the subscription) = the org
//! id, the buyer's user id and an HMAC of both under a secret of this server. The webhook proves
//! Stripe sent an event, not that the buyer may change the org it names, so an event takes its org
//! from the subscription (or customer) id already on an `entitlements` row, else from signed
//! metadata; neither = `unmatched` (recorded, nothing changed; `hitop-sync-server billing-events
//! --unmatched` lists them, `billing-link` ties one to an org).
//!
//! **Ordering.** Stripe doesn't deliver in order. Subscription events apply when their `created`
//! is not older than the last one applied for that subscription (`ls_updated_at`); older ones are
//! `stale`. Several events share a second at purchase time: on a tie the later delivery wins.
//! Invoice events apply only when not older than that mark and never move it; a refund does. An
//! event for another subscription than the org's takes over only when it is live and the org's own
//! is not (a new purchase after a refund or an expiry); otherwise it is `superseded` and ignored.
//!
//! **Status → limits** (`limits::Entitlement::plan_in_force`): Stripe's `active`, `trialing`
//! (stored as `on_trial`: `trialing` is this server's own trial), `past_due` and a subscription set
//! to cancel (`cancel_at_period_end` or `cancel_at`: stored as `cancelled`, until the period ends)
//! keep the plan; `unpaid`, `paused`, `incomplete`, `canceled` and `incomplete_expired` (stored as
//! `expired`) and `refunded` are Free limits. Nothing is ever deleted: creates beyond Free are
//! refused, reads and pulls always work. A full refund stays: later subscription events update the
//! ids and period but not the status, until a newer `invoice.paid` or
//! `customer.subscription.resumed` lifts it. A partial refund changes nothing.
//!
//! **Seats.** Team and Business are per seat: the subscription item's quantity is the seats
//! bought. `POST .../billing/seats` updates it: more seats are invoiced at once, prorated
//! (`proration_behavior=always_invoice`, `payment_behavior=error_if_incomplete`: a declined card
//! changes nothing), fewer take effect at once with no credit (`proration_behavior=none`: the lower
//! price from the next renewal), and never below the seats in use. Pro is one person.
//!
//! **Trials.** One 14-day Team trial per team org, per account and per email domain (common free
//! mail domains excepted), no card. The trial ends at `period_end` (`expire`, at every limit
//! check and hourly): the org drops to Free limits with nothing deleted. Buying replaces it.

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

pub const DEFAULT_API_URL: &str = "https://api.stripe.com";
/// The API version every call pins (and the webhook endpoint must use).
pub const STRIPE_VERSION: &str = "2026-10-28.endive";
/// How far a signature's `t` may be from now (Stripe's libraries use the same 5 minutes).
pub const SIGNATURE_TOLERANCE_SECS: i64 = 300;
/// Seats of a Team trial (a proposal, like every number in limits.rs).
pub const TRIAL_SEATS: i64 = 5;
const MAX_SEATS: i64 = 1000;

/// One purchasable price.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Price {
    pub plan: &'static str,
    pub period: &'static str,
    pub id: String,
}

/// (plan, period, Stripe lookup key) of everything sold through checkout.
pub const LOOKUP_KEYS: &[(&str, &str, &str)] = &[
    ("pro", "annual", "pro_yearly"),
    ("pro", "monthly", "pro_monthly"),
    ("team", "annual", "team_yearly"),
    ("team", "monthly", "team_monthly"),
    ("business", "annual", "business_yearly"),
    ("business", "monthly", "business_monthly"),
];
/// Prices found by lookup key, and when.
pub type LookedUp = std::sync::Arc<std::sync::Mutex<Option<(i64, Vec<Price>)>>>;
/// Form fields of a Stripe request.
type Form = Vec<(&'static str, String)>;
/// How long prices found by lookup key are kept.
const LOOKUP_TTL_SECS: i64 = 600;

#[derive(Clone)]
pub struct BillingConfig {
    pub secret_key: String,
    pub webhook_secret: String,
    /// Explicit price ids (`HITOP_SYNC_STRIPE_PRICE_*`); they win over lookup keys.
    pub prices: Vec<Price>,
    pub api_url: String,
    /// The key is a live key: only `livemode: true` events count (else only test ones).
    pub live: bool,
    /// Prices found by lookup key and when (shared by the clones of this config).
    pub looked_up: LookedUp,
}

impl std::fmt::Debug for BillingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secrets.
        f.debug_struct("BillingConfig").field("prices", &self.prices).field("api_url", &self.api_url).field("live", &self.live).finish()
    }
}

fn secret_of(var: &dyn Fn(&str) -> Option<String>, name: &str) -> Result<Option<String>, String> {
    match (var(name), var(&format!("{name}_FILE"))) {
        (Some(s), _) => Ok(Some(s.trim().to_string()).filter(|s| !s.is_empty())),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(&path).map_err(|e| format!("{name}_FILE {path}: {e}"))?.trim().to_string()).filter(|s| !s.is_empty())),
        (None, None) => Ok(None),
    }
}

impl BillingConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<BillingConfig>, String> {
        let key = secret_of(var, "HITOP_SYNC_STRIPE_SECRET_KEY")?;
        let hook = secret_of(var, "HITOP_SYNC_STRIPE_WEBHOOK_SECRET")?;
        let (secret_key, webhook_secret) = match (key, hook) {
            (None, None) => return Ok(None),
            (Some(k), Some(h)) => (k, h),
            _ => return Err("billing needs both HITOP_SYNC_STRIPE_SECRET_KEY and HITOP_SYNC_STRIPE_WEBHOOK_SECRET (or neither)".into()),
        };
        if !(secret_key.starts_with("sk_") || secret_key.starts_with("rk_")) {
            return Err("HITOP_SYNC_STRIPE_SECRET_KEY must be a Stripe secret key (sk_…) or restricted key (rk_…)".into());
        }
        if !webhook_secret.starts_with("whsec_") {
            return Err("HITOP_SYNC_STRIPE_WEBHOOK_SECRET must be the endpoint's signing secret (whsec_…)".into());
        }
        let mut prices = Vec::new();
        for (plan, period) in [("pro", "annual"), ("pro", "monthly"), ("team", "annual"), ("team", "monthly"), ("business", "annual"), ("business", "monthly")] {
            let name = format!("HITOP_SYNC_STRIPE_PRICE_{}_{}", plan.to_uppercase(), period.to_uppercase());
            if let Some(id) = var(&name).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
                if !id.starts_with("price_") {
                    return Err(format!("{name} must be a Stripe price id (price_…)"));
                }
                prices.push(Price { plan, period, id });
            }
        }
        let api_url = var("HITOP_SYNC_STRIPE_API_URL").unwrap_or_else(|| DEFAULT_API_URL.into()).trim_end_matches('/').to_string();
        if !crate::oidc::secure_url(&api_url) {
            return Err(format!("HITOP_SYNC_STRIPE_API_URL must be an https:// URL (http:// only on localhost): {api_url}"));
        }
        let live = secret_key.contains("_live_");
        Ok(Some(BillingConfig { secret_key, webhook_secret, prices, api_url, live, looked_up: Default::default() }))
    }

    /// A config with these explicit prices and nothing looked up yet.
    pub fn new(secret_key: &str, webhook_secret: &str, prices: Vec<Price>, api_url: &str, live: bool) -> BillingConfig {
        BillingConfig { secret_key: secret_key.into(), webhook_secret: webhook_secret.into(), prices, api_url: api_url.into(), live, looked_up: Default::default() }
    }

    /// The plan a subscription item's price sells: an explicit id, a looked-up id, or the lookup
    /// key the price object carries.
    fn plan_of(&self, price: &Json) -> Option<Price> {
        let id = ref_id(price);
        if let Some(p) = self.prices.iter().find(|p| p.id == id) {
            return Some(p.clone());
        }
        if let Some((_, found)) = self.looked_up.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Some(p) = found.iter().find(|p| p.id == id) {
                return Some(p.clone());
            }
        }
        let key = price["lookup_key"].as_str()?;
        LOOKUP_KEYS.iter().find(|(_, _, k)| *k == key).map(|(plan, period, _)| Price { plan, period, id })
    }
}

/// What is on sale: the explicit prices, then the lookup keys' prices for the rest (from Stripe,
/// cached). `Err` only when Stripe was needed and couldn't answer.
fn prices(state: &State, c: &BillingConfig) -> Result<Vec<Price>, ApiError> {
    let missing: Vec<&(&str, &str, &str)> = LOOKUP_KEYS.iter().filter(|(plan, period, _)| !c.prices.iter().any(|p| p.plan == *plan && p.period == *period)).collect();
    let mut all = c.prices.clone();
    if missing.is_empty() {
        return Ok(all);
    }
    let now = state.now();
    let cached = c.looked_up.lock().unwrap_or_else(|e| e.into_inner()).clone().filter(|(at, _)| now - at < LOOKUP_TTL_SECS && *at <= now);
    let found = match cached {
        Some((_, found)) => found,
        None => {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query.append_pair("active", "true").append_pair("limit", "100");
            for (_, _, key) in &missing {
                query.append_pair("lookup_keys[]", key);
            }
            let doc = stripe_call(state, c, "GET", &format!("/v1/prices?{}", query.finish()), &[])?;
            let found: Vec<Price> = doc["data"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter(|p| p["active"].as_bool() != Some(false) && p["recurring"].is_object())
                        .filter_map(|p| {
                            let key = p["lookup_key"].as_str()?;
                            let id = p["id"].as_str().filter(|id| id.starts_with("price_"))?;
                            LOOKUP_KEYS.iter().find(|(_, _, k)| *k == key).map(|(plan, period, _)| Price { plan, period, id: id.to_string() })
                        })
                        .collect()
                })
                .unwrap_or_default();
            *c.looked_up.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, found.clone()));
            found
        }
    };
    for (plan, period, _) in missing {
        if let Some(p) = found.iter().find(|p| p.plan == *plan && p.period == *period) {
            all.push(p.clone());
        }
    }
    // Always in LOOKUP_KEYS order (the portal lists them so).
    all.sort_by_key(|p| LOOKUP_KEYS.iter().position(|(plan, period, _)| *plan == p.plan && *period == p.period));
    Ok(all)
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

/// The `v1` signature Stripe puts in `Stripe-Signature` for `body` signed at `t`.
pub fn stripe_signature(secret: &str, t: i64, body: &[u8]) -> String {
    let mut payload = format!("{t}.").into_bytes();
    payload.extend_from_slice(body);
    hex(&hmac_sha256(secret.as_bytes(), &payload))
}

/// Stripe's `Stripe-Signature` check: one `t`, any `v1` matching, `t` within the tolerance of `now`.
pub fn signature_ok(secret: &str, body: &[u8], header: Option<&str>, now: i64) -> bool {
    let Some(h) = header else { return false };
    let mut t: Option<i64> = None;
    let mut v1 = Vec::new();
    for part in h.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => {
                if t.is_some() {
                    return false;
                }
                t = v.trim().parse().ok();
            }
            Some(("v1", v)) => v1.push(v.trim().to_ascii_lowercase()),
            _ => {}
        }
    }
    let Some(t) = t else { return false };
    if (now - t).abs() > SIGNATURE_TOLERANCE_SECS {
        return false;
    }
    let expected = stripe_signature(secret, t, body);
    // Every candidate is compared (no early exit on the first match).
    v1.iter().fold(false, |ok, s| same(expected.as_bytes(), s.as_bytes()) | ok)
}

fn custom_sig(tx: &mut Tx, org: &str, user: &str) -> Result<String, ApiError> {
    let key = crate::password::server_secret(tx, "billing-custom-data")?;
    Ok(hex(&hmac_sha256(&key, format!("hitop-billing/v1\n{org}\n{user}").as_bytes())))
}

/// A Stripe reference: an id string, or an expanded object's `id`.
fn ref_id(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Object(o) => o.get("id").and_then(Json::as_str).unwrap_or_default().to_string(),
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

fn customer_of(tx: &mut Tx, org: &str) -> Result<String, DbError> {
    Ok(tx.query_one("SELECT ls_customer_id FROM entitlements WHERE org_id = $1", &[org.into()])?.map(|r| r.text(0)).unwrap_or_default())
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

fn write_row(state: &State, tx: &mut Tx, org: &str, r: &Row, customer: Option<&str>, price: Option<&str>) -> Result<(), DbError> {
    ensure_row(state, tx, org)?;
    tx.execute(
        "UPDATE entitlements SET plan = $1, seats = $2, status = $3, period_end = $4, ls_subscription_id = $5, ls_item_id = $6, ls_order_id = $7, ls_updated_at = $8, updated_at = $9 WHERE org_id = $10",
        &[(&r.plan).into(), r.seats.into(), (&r.status).into(), r.period_end.into(), (&r.sub).into(), (&r.item).into(), (&r.order).into(), r.mark.into(), state.now().into(), org.into()],
    )?;
    if let Some(c) = customer.filter(|c| !c.is_empty()) {
        tx.execute("UPDATE entitlements SET ls_customer_id = $1 WHERE org_id = $2", &[c.into(), org.into()])?;
    }
    if let Some(p) = price.filter(|p| !p.is_empty()) {
        tx.execute("UPDATE entitlements SET ls_variant_id = $1 WHERE org_id = $2", &[p.into(), org.into()])?;
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
}

impl Outcome {
    fn new(outcome: &'static str) -> Outcome {
        Outcome { outcome, org: String::new(), subscription: String::new() }
    }

    fn of(outcome: &'static str, subscription: &str) -> Outcome {
        Outcome { outcome, org: String::new(), subscription: subscription.to_string() }
    }
}

/// `POST /v1/billing/stripe`: a Stripe webhook delivery (module docs).
pub fn webhook(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let c = config(state)?;
    let sig = request.header("stripe-signature").map(|h| h.value.to_string());
    if !signature_ok(&c.webhook_secret, &request.body, sig.as_deref(), state.now()) {
        return Err(ApiError::new(401, "bad_signature", "Stripe-Signature doesn't match the body (or is too old)"));
    }
    let doc: Json = serde_json::from_slice(&request.body).map_err(|_| ApiError::bad("the body isn't JSON"))?;
    let id = doc["id"].as_str().unwrap_or_default().to_string();
    if !id.starts_with("evt_") || doc["object"] != "event" {
        return Err(ApiError::bad("the body isn't a Stripe event"));
    }
    let event = doc["type"].as_str().unwrap_or_default().to_string();
    let mark = doc["created"].as_i64().unwrap_or(0).saturating_mul(1_000_000);
    let livemode = doc["livemode"].as_bool().unwrap_or(false);
    state.db.tx(|tx| {
        if tx.query_one("SELECT outcome FROM billing_events WHERE id = $1", &[(&id).into()])?.is_some() {
            return Ok((200, json!({ "outcome": "duplicate" })));
        }
        let o = if livemode != c.live { Outcome::new("livemode_mismatch") } else { apply(state, tx, c, &event, &doc["data"]["object"], mark)? };
        tx.execute(
            "INSERT INTO billing_events (id, event_name, org_id, subscription_id, resource_updated_at, test_mode, outcome, received_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[(&id).into(), (&event).into(), (&o.org).into(), (&o.subscription).into(), mark.into(), i64::from(!livemode).into(), o.outcome.into(), state.now().into()],
        )?;
        if o.outcome != "applied" {
            eprintln!("hitop-sync-server: billing: {event} {} {}: {}", id, if o.subscription.is_empty() { "-" } else { &o.subscription }, o.outcome);
        }
        Ok((200, json!({ "outcome": o.outcome, "org": if o.org.is_empty() { Json::Null } else { o.org.clone().into() } })))
    })
}

/// The org an event belongs to: the subscription's row, else verified metadata.
fn org_for(tx: &mut Tx, subscription: &str, meta: &Json) -> Result<Option<String>, ApiError> {
    if !subscription.is_empty() {
        if let Some(r) = tx.query_one("SELECT org_id FROM entitlements WHERE ls_subscription_id = $1", &[subscription.into()])? {
            return Ok(Some(r.text(0)));
        }
    }
    let text = |k: &str| meta[k].as_str().unwrap_or_default().to_string();
    let (org, user, sig) = (text("org_id"), text("user_id"), text("sig"));
    if org.is_empty() || user.is_empty() || sig.is_empty() {
        return Ok(None);
    }
    if !same(custom_sig(tx, &org, &user)?.as_bytes(), sig.as_bytes()) {
        return Ok(None);
    }
    Ok(tx.query_one("SELECT id FROM orgs WHERE id = $1", &[(&org).into()])?.map(|r| r.text(0)))
}

fn apply(state: &State, tx: &mut Tx, c: &BillingConfig, event: &str, obj: &Json, mark: i64) -> Result<Outcome, ApiError> {
    match event {
        "checkout.session.completed" => session_event(state, tx, obj),
        e if e.starts_with("customer.subscription.") && obj["object"] == "subscription" => subscription_event(state, tx, c, event, obj, mark),
        "invoice.paid" | "invoice.payment_failed" => invoice_event(state, tx, event, obj, mark),
        "charge.refunded" => refund_event(state, tx, event, obj, mark),
        _ => Ok(Outcome::new("ignored")),
    }
}

/// `checkout.session.completed`: the purchase's subscription and customer, for the signed org.
/// The plan itself comes from the subscription events (which may arrive before or after this).
fn session_event(state: &State, tx: &mut Tx, s: &Json) -> Result<Outcome, ApiError> {
    let sub = ref_id(&s["subscription"]);
    let mut out = Outcome::of("ignored", &sub);
    if s["mode"] != "subscription" || sub.is_empty() {
        return Ok(out);
    }
    let Some(org) = org_for(tx, &sub, &s["metadata"])? else {
        out.outcome = "unmatched";
        return Ok(out);
    };
    out.org = org.clone();
    let before = row(tx, &org)?;
    let mut after = row(tx, &org)?;
    if before.sub != sub {
        if !before.sub.is_empty() && LIVE.contains(&before.status.as_str()) {
            out.outcome = "superseded";
            return Ok(out);
        }
        // A new subscription (none yet, or the old one ended): its events will now match.
        after = Row { sub: sub.clone(), item: String::new(), mark: 0, ..after };
    }
    after.order = s["id"].as_str().unwrap_or_default().to_string();
    write_row(state, tx, &org, &after, Some(&ref_id(&s["customer"])), None)?;
    out.outcome = "applied";
    Ok(out)
}

/// Stripe's subscription status → ours (module docs).
fn status_of(sub: &Json) -> &'static str {
    let cancelling = sub["cancel_at_period_end"].as_bool() == Some(true) || sub["cancel_at"].as_i64().is_some();
    match sub["status"].as_str().unwrap_or_default() {
        "active" | "trialing" if cancelling => "cancelled",
        "active" => "active",
        "trialing" => "on_trial",
        "past_due" => "past_due",
        "unpaid" => "unpaid",
        "paused" => "paused",
        "canceled" | "incomplete_expired" => "expired",
        // `incomplete` (the first payment didn't go through yet) and anything new: no plan.
        _ => "incomplete",
    }
}

fn subscription_event(state: &State, tx: &mut Tx, c: &BillingConfig, event: &str, s: &Json, mark: i64) -> Result<Outcome, ApiError> {
    let sub = s["id"].as_str().unwrap_or_default().to_string();
    let mut out = Outcome::of("applied", &sub);
    let Some(org) = org_for(tx, &sub, &s["metadata"])? else {
        out.outcome = "unmatched";
        return Ok(out);
    };
    out.org = org.clone();
    let item = &s["items"]["data"][0];
    let price_id = ref_id(&item["price"]);
    let Some(price) = c.plan_of(&item["price"]) else {
        out.outcome = "unknown_price";
        return Ok(out);
    };
    let status = status_of(s);
    let before = row(tx, &org)?;
    if !before.sub.is_empty() && before.sub != sub {
        // Another subscription: a live one replaces the org's only while the org's own isn't
        // running (a new purchase after a refund or expiry); a lapsed one never does, and neither
        // does a late event of an old subscription once a newer one runs.
        if !LIVE.contains(&status) || LIVE.contains(&before.status.as_str()) {
            out.outcome = "superseded";
            return Ok(out);
        }
    } else if mark < before.mark {
        out.outcome = "stale";
        return Ok(out);
    }
    let same_sub = before.sub == sub;
    let period = item["current_period_end"].as_i64();
    let period_end = match status {
        "cancelled" => s["cancel_at"].as_i64().or(period),
        "expired" => s["ended_at"].as_i64().or(period),
        _ => period,
    };
    let mut after = Row {
        plan: price.plan.to_string(),
        seats: if per_seat(price.plan) { item["quantity"].as_i64().unwrap_or(before.seats).clamp(1, MAX_SEATS) } else { 1 },
        status: status.to_string(),
        period_end: period_end.unwrap_or(before.period_end),
        sub: sub.clone(),
        item: Some(item["id"].as_str().unwrap_or_default().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| if same_sub { before.item.clone() } else { String::new() }),
        order: if same_sub { before.order.clone() } else { String::new() },
        mark: mark.max(if same_sub { before.mark } else { 0 }),
    };
    // A refund stays until a resume or a newer paid invoice (module docs).
    if same_sub && before.status == "refunded" && event != "customer.subscription.resumed" {
        after.status = "refunded".into();
    }
    write_row(state, tx, &org, &after, Some(&ref_id(&s["customer"])), Some(&price_id))?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    Ok(out)
}

fn invoice_event(state: &State, tx: &mut Tx, event: &str, inv: &Json, mark: i64) -> Result<Outcome, ApiError> {
    let details = &inv["parent"]["subscription_details"];
    let sub = Some(ref_id(&details["subscription"])).filter(|s| !s.is_empty()).unwrap_or_else(|| ref_id(&inv["subscription"]));
    let mut out = Outcome::of("applied", &sub);
    if sub.is_empty() {
        out.outcome = "ignored";
        return Ok(out);
    }
    let Some(org) = org_for(tx, &sub, &details["metadata"])? else {
        out.outcome = "unmatched";
        return Ok(out);
    };
    out.org = org.clone();
    let before = row(tx, &org)?;
    if before.sub != sub {
        out.outcome = "superseded";
        return Ok(out);
    }
    if mark < before.mark {
        out.outcome = "stale";
        return Ok(out);
    }
    let mut after = row(tx, &org)?;
    match event {
        "invoice.payment_failed" if matches!(before.status.as_str(), "active" | "on_trial") => after.status = "past_due".into(),
        "invoice.paid" if matches!(before.status.as_str(), "past_due" | "unpaid" | "refunded") => after.status = "active".into(),
        _ => {
            out.outcome = "ignored";
            return Ok(out);
        }
    }
    write_row(state, tx, &org, &after, None, None)?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    Ok(out)
}

/// `charge.refunded`: a full refund (by the owner, or by Stripe/Link support) ends the plan.
fn refund_event(state: &State, tx: &mut Tx, event: &str, charge: &Json, mark: i64) -> Result<Outcome, ApiError> {
    let customer = ref_id(&charge["customer"]);
    let mut out = Outcome::new("unmatched");
    let found = if customer.is_empty() { None } else { tx.query_one("SELECT org_id, ls_subscription_id FROM entitlements WHERE ls_customer_id = $1", &[(&customer).into()])? };
    let Some(r) = found else { return Ok(out) };
    let org = r.text(0);
    out.org = org.clone();
    out.subscription = r.text(1);
    let amount = charge["amount"].as_i64().unwrap_or(0);
    let full = charge["refunded"].as_bool() == Some(true) || (amount > 0 && charge["amount_refunded"].as_i64().unwrap_or(0) >= amount);
    if !full {
        out.outcome = "ignored";
        return Ok(out);
    }
    let before = row(tx, &org)?;
    let after = Row { status: "refunded".into(), mark: before.mark.max(mark), ..row(tx, &org)? };
    write_row(state, tx, &org, &after, None, None)?;
    record_change(state, tx, &org, &before, &after, "billing", event_name(event), None)?;
    out.outcome = "applied";
    Ok(out)
}

/// Event names as `&'static str` for the audit detail (unknown ones as "other").
fn event_name(e: &str) -> &'static str {
    const KNOWN: &[&str] = &[
        "checkout.session.completed",
        "customer.subscription.created",
        "customer.subscription.updated",
        "customer.subscription.deleted",
        "customer.subscription.paused",
        "customer.subscription.resumed",
        "invoice.paid",
        "invoice.payment_failed",
        "charge.refunded",
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

/// Team and Business are sold per seat (the subscription item's quantity).
pub fn per_seat(plan: &str) -> bool {
    matches!(plan, "team" | "business")
}

fn offers(on_sale: &[Price], personal: bool) -> Vec<Json> {
    let plans: &[&str] = if personal { &["pro"] } else { &["team", "business"] };
    on_sale.iter().filter(|p| plans.contains(&p.plan)).map(|p| json!({ "plan": p.plan, "period": p.period })).collect()
}

/// A Stripe subscription that is still running (perhaps unpaid or paused): change it in the
/// customer portal rather than buying a second one.
fn subscribed(r: &Row) -> bool {
    !r.sub.is_empty() && matches!(r.status.as_str(), "active" | "on_trial" | "past_due" | "cancelled" | "paused" | "unpaid")
}

const FREE_MAIL: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com", "yahoo.com", "icloud.com", "me.com", "proton.me", "protonmail.com", "gmx.de", "gmx.net", "web.de", "mail.ru", "yandex.ru", "aol.com", "fastmail.com", "zoho.com", "example.com",
];

/// A public mail provider's domain (no company owns it).
pub fn free_mail(domain: &str) -> bool {
    FREE_MAIL.contains(&domain)
}

fn trial_domain(email: &str) -> String {
    let d = email.rsplit_once('@').map(|(_, d)| d.to_lowercase()).unwrap_or_default();
    if free_mail(&d) {
        String::new()
    } else {
        d
    }
}

/// Whether `user` may start a trial for `org` (None) or why not.
fn trial_refusal(state: &State, tx: &mut Tx, org: &orgs::Org, user: &User, r: &Row) -> Result<Option<&'static str>, DbError> {
    if !state.config.plan_limits || state.config.billing.is_none() || state.config.trial_days <= 0 {
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
    // What is on sale (perhaps a Stripe call, so before the transaction); without Stripe's
    // answer only the explicit prices are offered.
    let on_sale = state.config.billing.as_ref().map(|c| prices(state, c).unwrap_or_else(|_| c.prices.clone())).unwrap_or_default();
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
                "checkout": c.is_some(),
                "testMode": c.is_some_and(|c| !c.live),
                "plan": r.plan,
                "planInForce": e.plan_in_force(),
                "status": r.status,
                "seats": r.seats,
                "seatsUsed": used,
                "periodEnd": r.period_end,
                "pastDue": r.status == "past_due",
                "subscribed": subscribed(&r),
                "offers": offers(&on_sale, o.personal),
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

/// One Stripe API call (form-encoded, pinned version), outside any database transaction.
fn stripe_call(state: &State, c: &BillingConfig, method: &str, path: &str, form: &[(&str, String)]) -> Result<Json, ApiError> {
    let url = format!("{}{path}", c.api_url);
    let auth = format!("Bearer {}", c.secret_key);
    let result = if method == "GET" {
        state.billing_http.get(&url).header("Authorization", &auth).header("Stripe-Version", STRIPE_VERSION).call()
    } else {
        let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(form.iter().map(|(k, v)| (*k, v.as_str()))).finish();
        let idempotency = uuid::Uuid::new_v4().to_string();
        state
            .billing_http
            .post(&url)
            .header("Authorization", &auth)
            .header("Stripe-Version", STRIPE_VERSION)
            .header("Idempotency-Key", &idempotency)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send(body)
    };
    let unreachable = |e: String| {
        eprintln!("hitop-sync-server: Stripe: {e}");
        ApiError::new(503, "billing_unreachable", "Stripe is unreachable; try again")
    };
    let mut res = result.map_err(|e| unreachable(format!("{method} {path}: {e}")))?;
    let status = res.status().as_u16();
    let text = res.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| unreachable(format!("{method} {path}: {e}")))?;
    let doc: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
    if !(200..300).contains(&status) {
        let message = doc["error"]["message"].as_str().unwrap_or("").to_string();
        eprintln!("hitop-sync-server: Stripe {method} {path}: {status} {}", doc["error"]["code"].as_str().unwrap_or(""));
        return Err(ApiError::new(502, "billing_refused", format!("Stripe refused the request ({status}){}", if message.is_empty() { String::new() } else { format!(": {message}") })));
    }
    Ok(doc)
}

/// `POST /v1/orgs/:org/billing/checkout {"plan","period","seats"}` (admin) → `{url}`: a Stripe
/// Checkout Session with Managed Payments (Pro for a personal org, Team or Business per seat for
/// a team org).
pub fn checkout(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let input: CheckoutIn = body(request)?;
    let period = input.period.unwrap_or_else(|| "annual".into());
    // An explicit price needs no Stripe call; anything else is found by lookup key.
    let on_sale = if c.prices.iter().any(|p| p.plan == input.plan && p.period == period) { c.prices.clone() } else { prices(state, c)? };
    let (user, form) = state.db.tx(|tx| -> Result<(User, Form), ApiError> {
        let (user, o) = caller(state, tx, request, &org, OrgRole::Admin)?;
        let allowed = if o.personal { input.plan == "pro" } else { per_seat(&input.plan) };
        if !allowed {
            return Err(ApiError::bad(if o.personal { "a personal account buys Pro; for Team or Business, create an organization" } else { "an organization buys Team or Business (per seat); Pro is for one person's personal account" }));
        }
        let price = on_sale.iter().find(|p| p.plan == input.plan && p.period == period).ok_or_else(|| ApiError::bad(format!("{} {period} isn't on sale here", input.plan)))?;
        let r = row(tx, &org)?;
        if subscribed(&r) {
            return Err(ApiError::conflict("already_subscribed", "this organization has a subscription: change it in the billing portal", json!({})));
        }
        let used = limits::usage(state, tx, &org)?.seats.max(1);
        let quantity = if per_seat(&input.plan) { input.seats.unwrap_or(used) } else { 1 };
        if !(1..=MAX_SEATS).contains(&quantity) {
            return Err(ApiError::bad(format!("seats must be 1 to {MAX_SEATS}")));
        }
        if quantity < used {
            return Err(ApiError::bad(format!("the organization uses {used} seat(s): buy at least that many")));
        }
        let sig = custom_sig(tx, &org, &user.id)?;
        let back = format!("{}#/orgs/{org}/plan", state.config.portal_url);
        let form = vec![
            ("mode", "subscription".to_string()),
            ("managed_payments[enabled]", "true".to_string()),
            ("line_items[0][price]", price.id.clone()),
            ("line_items[0][quantity]", quantity.to_string()),
            // The plan page shows the new plan once the webhook has applied it (the hash router
            // takes no query, so none is added).
            ("success_url", back.clone()),
            ("cancel_url", back),
            ("customer_email", user.email.clone()),
            ("client_reference_id", org.clone()),
            ("metadata[org_id]", org.clone()),
            ("metadata[user_id]", user.id.clone()),
            ("metadata[sig]", sig.clone()),
            ("subscription_data[metadata][org_id]", org.clone()),
            ("subscription_data[metadata][user_id]", user.id.clone()),
            ("subscription_data[metadata][sig]", sig),
        ];
        Ok((user, form))
    })?;
    let doc = stripe_call(state, c, "POST", "/v1/checkout/sessions", &form)?;
    let url = doc["url"].as_str().ok_or_else(|| ApiError::new(502, "billing_refused", "Stripe sent no checkout URL"))?.to_string();
    state.db.tx(|tx| audit::record(tx, state, Event::new(&orgs::chain(&org), "checkout_started", Some(&user)).target(&org).detail(json!({ "plan": input.plan, "period": period }))))?;
    Ok((200, json!({ "url": url })))
}

/// `GET /v1/orgs/:org/billing/portal` (admin) → `{url}`: a Stripe customer portal session for the
/// org's customer (card, invoices, cancel, monthly/annual), a fresh short-lived link each time.
pub fn portal(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let customer = state.db.tx(|tx| -> Result<String, ApiError> {
        caller(state, tx, request, &org, OrgRole::Admin)?;
        let customer = customer_of(tx, &org)?;
        if customer.is_empty() {
            return Err(ApiError::conflict("no_subscription", "this organization has no subscription yet", json!({})));
        }
        Ok(customer)
    })?;
    let doc = stripe_call(state, c, "POST", "/v1/billing_portal/sessions", &[("customer", customer), ("return_url", format!("{}#/orgs/{org}/plan", state.config.portal_url))])?;
    let url = doc["url"].as_str().ok_or_else(|| ApiError::new(502, "billing_refused", "Stripe sent no customer portal URL"))?;
    Ok((200, json!({ "url": url })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeatsIn {
    seats: i64,
}

/// `POST /v1/orgs/:org/billing/seats {"seats"}` (admin): Team or Business seats bought (module docs).
pub fn seats(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let org = params.get("org").unwrap_or_default().to_string();
    let input: SeatsIn = body(request)?;
    let (user, sub, item, before) = state.db.tx(|tx| -> Result<(User, String, String, i64), ApiError> {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Admin)?;
        let r = row(tx, &org)?;
        if !per_seat(&r.plan) || r.sub.is_empty() || r.item.is_empty() || !matches!(r.status.as_str(), "active" | "on_trial" | "past_due") {
            return Err(ApiError::conflict("no_subscription", "seats change on a live Team or Business subscription", json!({})));
        }
        let used = limits::usage(state, tx, &org)?.seats;
        if !(1..=MAX_SEATS).contains(&input.seats) {
            return Err(ApiError::bad(format!("seats must be 1 to {MAX_SEATS}")));
        }
        if input.seats < used {
            return Err(ApiError::bad(format!("the organization uses {used} seat(s): remove people first")));
        }
        Ok((user, r.sub, r.item, r.seats))
    })?;
    if input.seats == before {
        return Ok((200, json!({ "org": org, "seats": before })));
    }
    let mut form = vec![("items[0][id]", item.clone()), ("items[0][quantity]", input.seats.to_string())];
    if input.seats > before {
        form.push(("proration_behavior", "always_invoice".into()));
        form.push(("payment_behavior", "error_if_incomplete".into()));
    } else {
        form.push(("proration_behavior", "none".into()));
    }
    let doc = stripe_call(state, c, "POST", &format!("/v1/subscriptions/{sub}"), &form)?;
    let quantity = doc["items"]["data"].as_array().and_then(|items| items.iter().find(|i| i["id"] == item.as_str())).and_then(|i| i["quantity"].as_i64()).unwrap_or(input.seats).clamp(1, MAX_SEATS);
    state.db.tx(|tx| -> Result<(), ApiError> {
        let b = row(tx, &org)?;
        if b.item != item {
            return Ok(());
        }
        // The mark stays: Stripe's `customer.subscription.updated` for this change follows.
        let after = Row { seats: quantity, ..row(tx, &org)? };
        write_row(state, tx, &org, &after, None, None)?;
        record_change(state, tx, &org, &b, &after, "seats", "seats_changed", Some(&user))?;
        Ok(())
    })?;
    Ok((200, json!({ "org": org, "seats": quantity })))
}

/// `hitop-sync-server billing-events [--unmatched]`: the latest deliveries (ids and outcomes).
pub fn list_events(state: &State, unmatched: bool) -> Result<Vec<Json>, DbError> {
    state.db.read_tx(|tx| {
        // No parameter compared with a literal: Postgres would type it int4 and refuse an i64.
        let filter = if unmatched { "WHERE outcome = 'unmatched' " } else { "" };
        let rows = tx.query(&format!("SELECT id, event_name, org_id, subscription_id, outcome, test_mode, received_at FROM billing_events {filter}ORDER BY received_at DESC, id LIMIT 200"), &[])?;
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
    fn stripe_signatures() {
        let body = br#"{"id":"evt_1","object":"event"}"#;
        let now = 1_800_000_000;
        let good = stripe_signature("whsec_s3cret", now, body);
        let h = |s: &str| Some(s.to_string());
        assert!(signature_ok("whsec_s3cret", body, h(&format!("t={now},v1={good}")).as_deref(), now));
        // Order doesn't matter, v0 and unknown parts are skipped, a second v1 (secret rolling) is fine.
        assert!(signature_ok("whsec_s3cret", body, h(&format!("v0=abc,v1={},t={now},v1={good},x=1", "0".repeat(64))).as_deref(), now + 299));
        assert!(!signature_ok("whsec_s3cret", body, None, now));
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("v1={good}")).as_deref(), now), "no t");
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("t={now},t={now},v1={good}")).as_deref(), now), "two t");
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("t={now},v1={good}")).as_deref(), now + 301), "too old");
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("t={now},v1={good}")).as_deref(), now - 301), "from the future");
        assert!(!signature_ok("whsec_other", body, h(&format!("t={now},v1={good}")).as_deref(), now));
        assert!(!signature_ok("whsec_s3cret", b"{}", h(&format!("t={now},v1={good}")).as_deref(), now));
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("t={},v1={good}", now + 1)).as_deref(), now), "t is signed");
        assert!(!signature_ok("whsec_s3cret", body, h(&format!("t={now},v0={good}")).as_deref(), now), "only v1 counts");
        // Stripe's documented construction: HMAC-SHA256 over "<t>.<body>" with the secret as given.
        assert_eq!(good, hex(&hmac_sha256(b"whsec_s3cret", format!("{now}.{}", std::str::from_utf8(body).unwrap()).as_bytes())));
    }

    #[test]
    fn stripe_statuses() {
        let s = |v: Json| status_of(&v);
        assert_eq!(s(json!({ "status": "active" })), "active");
        assert_eq!(s(json!({ "status": "active", "cancel_at_period_end": true })), "cancelled");
        assert_eq!(s(json!({ "status": "active", "cancel_at": 1_800_000_000 })), "cancelled");
        assert_eq!(s(json!({ "status": "trialing" })), "on_trial", "never our own trial's status");
        assert_eq!(s(json!({ "status": "past_due", "cancel_at_period_end": true })), "past_due", "the banner wins");
        assert_eq!(s(json!({ "status": "canceled" })), "expired");
        assert_eq!(s(json!({ "status": "incomplete_expired" })), "expired");
        assert_eq!(s(json!({ "status": "incomplete" })), "incomplete");
        assert_eq!(s(json!({ "status": "something_new" })), "incomplete");
        for (stripe, ours) in [("unpaid", "unpaid"), ("paused", "paused")] {
            assert_eq!(s(json!({ "status": stripe })), ours);
        }
    }

    #[test]
    fn config_from_env() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert!(BillingConfig::from_env(&env(&[])).unwrap().is_none(), "off when unset");
        assert!(BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_x")])).is_err(), "one without the other");
        let base: &[(&str, &str)] = &[("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y"), ("HITOP_SYNC_STRIPE_PRICE_TEAM_ANNUAL", "price_t")];
        let c = BillingConfig::from_env(&env(base)).unwrap().unwrap();
        assert_eq!((c.live, c.prices.len(), c.api_url.as_str()), (false, 1, DEFAULT_API_URL));
        assert!(!format!("{c:?}").contains("sk_test_x") && !format!("{c:?}").contains("whsec_y"), "Debug never shows secrets");
        let live = BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "rk_live_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y"), ("HITOP_SYNC_STRIPE_PRICE_PRO_MONTHLY", "price_p")])).unwrap().unwrap();
        assert!(live.live);
        let bare = BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y")])).unwrap().unwrap();
        assert!(bare.prices.is_empty(), "the two secrets are enough: prices by lookup key");
        let p = |id: &str, key: &str| bare.plan_of(&json!({ "id": id, "lookup_key": key }));
        assert_eq!(p("price_x", "team_yearly"), Some(Price { plan: "team", period: "annual", id: "price_x".into() }));
        assert_eq!(p("price_y", "business_monthly").map(|p| p.plan), Some("business"));
        assert_eq!(p("price_z", "something_else"), None);
        assert_eq!(c.plan_of(&json!("price_t")).map(|p| p.plan), Some("team"), "an explicit id, unexpanded");
        assert!(BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y"), ("HITOP_SYNC_STRIPE_PRICE_PRO_MONTHLY", "101")])).is_err(), "not a price id");
        assert!(BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "pk_test_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y"), ("HITOP_SYNC_STRIPE_PRICE_PRO_MONTHLY", "price_p")])).is_err(), "a publishable key");
        assert!(BillingConfig::from_env(&env(&[("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_test_x"), ("HITOP_SYNC_STRIPE_WEBHOOK_SECRET", "whsec_y"), ("HITOP_SYNC_STRIPE_PRICE_PRO_MONTHLY", "price_p"), ("HITOP_SYNC_STRIPE_API_URL", "http://api.example.com")])).is_err());
    }

    #[test]
    fn trial_domains_skip_free_mail() {
        assert_eq!(trial_domain("a@Gmail.com"), "");
        assert_eq!(trial_domain("a@acme.io"), "acme.io");
    }
}
