//! Plans, entitlements, usage and the one place limits are enforced (HITOP Cloud,
//! spec/CLOUD_SERVICE.md sections 3 and 4 "Metering and quotas").
//!
//! Every org has one `entitlements` row: its plan (`free`, `pro`, `team`, `business`,
//! `enterprise`), bought seats, a JSON object of per-limit overrides and a status. Entitlements are
//! the only thing the rest of the server reads; billing (C3) will write them. The numbers below are
//! the plan's proposal until the owner sets prices; changing one here changes what is enforced.
//!
//! Limits are enforced only with `HITOP_SYNC_LIMITS=plans` (HITOP Cloud). A self-hosted server has
//! no plans, seats or per-org limits: every check here is a no-op there (the server-wide limits in
//! config.rs still apply). What is refused is always a **create**: a new workspace, person, device,
//! runner credential, invite, or a write that makes the org's storage grow. Reads, pulls, deletes
//! and writes that keep or shrink the stored bytes are never refused, so a lapsed or downgraded
//! plan never takes data hostage. A refusal is a 402 with `{error: "limit", limit, plan, org,
//! used, max, upgradeUrl}` and a message that names all of it (released clients print the
//! message as it is).
//!
//! Grandfathering: migration 4 gives every existing account a personal org on Free; `grandfather`
//! then writes the account's current usage as overrides wherever it is above Free, so nobody is
//! over a limit on day one (`over_limits` lists anyone who is; it must be empty).
//!
//! Seats: a seat is a person (never a runner credential) who is a member of the org, has a pending
//! invite to it, or is a member of one of its workspaces. People whose only role in the org's
//! workspaces is viewer and who aren't org members are guests: the plan's free guest viewers
//! (Team: 5) don't use a seat, the rest do.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value as Json};

use crate::api::{ApiError, State};
use crate::db::{DbError, Tx};

pub const PLANS: &[&str] = &["free", "pro", "team", "business", "enterprise"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Seats,
    Workspaces,
    StorageBytes,
    Devices,
    Runners,
    InvitesPerDay,
}

impl Metric {
    pub const ALL: [Metric; 6] = [Metric::Seats, Metric::Workspaces, Metric::StorageBytes, Metric::Devices, Metric::Runners, Metric::InvitesPerDay];

    /// The name in the API (`limit` in errors, keys of `limits` and `usage`).
    pub fn name(self) -> &'static str {
        match self {
            Metric::Seats => "seats",
            Metric::Workspaces => "workspaces",
            Metric::StorageBytes => "storageBytes",
            Metric::Devices => "devices",
            Metric::Runners => "runners",
            Metric::InvitesPerDay => "invitesPerDay",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Metric::Seats => "people (seats)",
            Metric::Workspaces => "shared workspaces",
            Metric::StorageBytes => "bytes of encrypted storage",
            Metric::Devices => "devices per person",
            Metric::Runners => "runner credentials",
            Metric::InvitesPerDay => "invites a day",
        }
    }
}

const MB: i64 = 1 << 20;
const GB: i64 = 1 << 30;

/// Limits as numbers; `None` = no limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub seats: Option<i64>,
    /// Viewer-only people who don't use a seat.
    pub guest_viewers: i64,
    pub workspaces: Option<i64>,
    pub storage_bytes: Option<i64>,
    pub devices: Option<i64>,
    pub runners: Option<i64>,
    pub invites_per_day: Option<i64>,
}

impl Limits {
    pub fn get(&self, m: Metric) -> Option<i64> {
        match m {
            Metric::Seats => self.seats,
            Metric::Workspaces => self.workspaces,
            Metric::StorageBytes => self.storage_bytes,
            Metric::Devices => self.devices,
            Metric::Runners => self.runners,
            Metric::InvitesPerDay => self.invites_per_day,
        }
    }

    fn set(&mut self, m: Metric, v: Option<i64>) {
        match m {
            Metric::Seats => self.seats = v,
            Metric::Workspaces => self.workspaces = v,
            Metric::StorageBytes => self.storage_bytes = v,
            Metric::Devices => self.devices = v,
            Metric::Runners => self.runners = v,
            Metric::InvitesPerDay => self.invites_per_day = v,
        }
    }

    pub fn json(&self) -> Json {
        let mut m = Map::new();
        for metric in Metric::ALL {
            m.insert(metric.name().into(), self.get(metric).map_or(Json::Null, Json::from));
        }
        m.insert("guestViewers".into(), self.guest_viewers.into());
        Json::Object(m)
    }
}

/// A plan's limits (section 3), for `seats` bought seats (Team and Business). Invites a day are
/// not in section 3: they are a ceiling against abuse, not a price.
pub fn plan_limits(plan: &str, seats: i64) -> Limits {
    let s = seats.max(1);
    match plan {
        "pro" => Limits { seats: Some(3), guest_viewers: 0, workspaces: Some(25), storage_bytes: Some(2 * GB), devices: Some(10), runners: Some(10), invites_per_day: Some(50) },
        "team" => Limits { seats: Some(s), guest_viewers: 5, workspaces: None, storage_bytes: Some(10 * GB + s * GB), devices: Some(10), runners: Some(10 * s), invites_per_day: Some(200) },
        "business" => Limits { seats: Some(s), guest_viewers: 0, workspaces: None, storage_bytes: Some(50 * GB + 2 * s * GB), devices: Some(10), runners: Some(25 * s), invites_per_day: Some(1000) },
        "enterprise" => Limits { seats: None, guest_viewers: 0, workspaces: None, storage_bytes: None, devices: None, runners: None, invites_per_day: None },
        _ => Limits { seats: Some(3), guest_viewers: 0, workspaces: Some(3), storage_bytes: Some(100 * MB), devices: Some(3), runners: Some(5), invites_per_day: Some(20) },
    }
}

/// One org's `entitlements` row.
#[derive(Clone, Debug)]
pub struct Entitlement {
    pub org: String,
    pub plan: String,
    pub seats: i64,
    /// Overrides: on Enterprise the contract's numbers, on other plans floors (grandfathering).
    pub overrides: Map<String, Json>,
    pub status: String,
    pub period_end: i64,
}

impl Entitlement {
    /// The plan whose limits apply: a lapsed, paused, refunded or not yet paid (`incomplete`)
    /// subscription is on Free (nothing deleted). An ended trial or cancelled period becomes `expired` (`billing::expire`).
    pub fn plan_in_force(&self) -> &str {
        if matches!(self.status.as_str(), "expired" | "unpaid" | "paused" | "refunded" | "incomplete") {
            "free"
        } else {
            &self.plan
        }
    }

    pub fn limits(&self) -> Limits {
        let plan = self.plan_in_force();
        let mut l = plan_limits(plan, self.seats);
        for m in Metric::ALL {
            if let Some(v) = self.overrides.get(m.name()).and_then(Json::as_i64) {
                let next = match (plan, l.get(m)) {
                    ("enterprise", _) => Some(v),
                    (_, None) => None,
                    (_, Some(c)) => Some(c.max(v)),
                };
                l.set(m, next);
            }
        }
        l
    }
}

pub fn entitlement(tx: &mut Tx, org: &str) -> Result<Entitlement, DbError> {
    let row = tx.query_one("SELECT plan, seats, limits, status, period_end FROM entitlements WHERE org_id = $1", &[org.into()])?;
    Ok(match row {
        Some(r) => Entitlement {
            org: org.into(),
            plan: r.text(0),
            seats: r.int(1),
            overrides: serde_json::from_str::<Json>(&r.text(2)).ok().and_then(|j| j.as_object().cloned()).unwrap_or_default(),
            status: r.text(3),
            period_end: r.int(4),
        },
        None => Entitlement { org: org.into(), plan: "free".into(), seats: 0, overrides: Map::new(), status: "active".into(), period_end: 0 },
    })
}

/// The people an org counts (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct People {
    /// User ids of org members and of people with an editor or owner role in its workspaces, and
    /// `invite:<email>` for pending org invites.
    pub core: BTreeSet<String>,
    /// Viewer-only people in its workspaces who aren't in `core`.
    pub guests: BTreeSet<String>,
}

impl People {
    pub fn add(&mut self, person: &str, editor: bool) {
        if editor {
            self.guests.remove(person);
            self.core.insert(person.into());
        } else if !self.core.contains(person) {
            self.guests.insert(person.into());
        }
    }

    pub fn seats_used(&self, guest_viewers: i64) -> i64 {
        self.core.len() as i64 + (self.guests.len() as i64 - guest_viewers).max(0)
    }
}

pub fn people(tx: &mut Tx, org: &str) -> Result<People, DbError> {
    let mut p = People::default();
    for r in tx.query("SELECT user_id FROM org_members WHERE org_id = $1", &[org.into()])? {
        p.add(&r.text(0), true);
    }
    for r in tx.query("SELECT email FROM org_invites WHERE org_id = $1", &[org.into()])? {
        p.add(&format!("invite:{}", r.text(0)), true);
    }
    let rows = tx.query(
        "SELECT m.user_id, MAX(CASE WHEN m.role = 'viewer' THEN 0 ELSE 1 END) FROM members m JOIN workspaces w ON w.id = m.workspace_id JOIN users u ON u.id = m.user_id
         WHERE w.org_id = $1 AND u.runner_of = '' GROUP BY m.user_id",
        &[org.into()],
    )?;
    for r in rows {
        p.add(&r.text(0), r.int(1) != 0);
    }
    Ok(p)
}

/// What an org uses now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub seats: i64,
    pub guests: i64,
    pub workspaces: i64,
    pub storage_bytes: i64,
    pub runners: i64,
    /// The most devices any org member has.
    pub devices_max: i64,
    pub invites_today: i64,
    pub writes_today: i64,
}

pub fn day(state: &State) -> i64 {
    state.now().div_euclid(86_400)
}

fn counter(tx: &mut Tx, org: &str, metric: &str, day: i64) -> Result<i64, DbError> {
    Ok(tx.query_one("SELECT count FROM usage_counters WHERE org_id = $1 AND metric = $2 AND day = $3", &[org.into(), metric.into(), day.into()])?.map_or(0, |r| r.int(0)))
}

/// Adds `n` to today's counter of `metric` (`invites`, `writes`) for `org` (metering, also when
/// limits are off).
pub fn meter(state: &State, tx: &mut Tx, org: &str, metric: &str, n: i64) -> Result<(), DbError> {
    if org.is_empty() {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO usage_counters (org_id, metric, day, count) VALUES ($1, $2, $3, $4) ON CONFLICT (org_id, metric, day) DO UPDATE SET count = usage_counters.count + excluded.count",
        &[org.into(), metric.into(), day(state).into(), n.into()],
    )?;
    Ok(())
}

pub fn usage(state: &State, tx: &mut Tx, org: &str) -> Result<Usage, DbError> {
    crate::billing::expire(state, tx, Some(org))?;
    let e = entitlement(tx, org)?;
    let p = people(tx, org)?;
    let ws = tx.query_one("SELECT COUNT(*), CAST(COALESCE(SUM(bytes), 0) AS BIGINT) FROM workspaces WHERE org_id = $1", &[org.into()])?;
    let runners = tx.query_one("SELECT COUNT(*) FROM users u JOIN workspaces w ON w.id = u.runner_of WHERE w.org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0));
    let devices_max = tx
        .query_one(
            "SELECT CAST(COALESCE(MAX(n), 0) AS BIGINT) FROM (SELECT COUNT(*) AS n FROM device_keys k JOIN org_members o ON o.user_id = k.user_id WHERE o.org_id = $1 GROUP BY k.user_id) d",
            &[org.into()],
        )?
        .map_or(0, |r| r.int(0));
    let today = day(state);
    Ok(Usage {
        seats: p.seats_used(e.limits().guest_viewers),
        guests: p.guests.len() as i64,
        workspaces: ws.as_ref().map_or(0, |r| r.int(0)),
        storage_bytes: ws.as_ref().map_or(0, |r| r.int(1)),
        runners,
        devices_max,
        invites_today: counter(tx, org, "invites", today)?,
        writes_today: counter(tx, org, "writes", today)?,
    })
}

impl Usage {
    pub fn get(&self, m: Metric) -> i64 {
        match m {
            Metric::Seats => self.seats,
            Metric::Workspaces => self.workspaces,
            Metric::StorageBytes => self.storage_bytes,
            Metric::Devices => self.devices_max,
            Metric::Runners => self.runners,
            Metric::InvitesPerDay => self.invites_today,
        }
    }

    pub fn json(&self) -> Json {
        json!({
            "seats": self.seats,
            "guests": self.guests,
            "workspaces": self.workspaces,
            "storageBytes": self.storage_bytes,
            "devices": self.devices_max,
            "runners": self.runners,
            "invitesPerDay": self.invites_today,
            "writesToday": self.writes_today,
        })
    }
}

/// The org a workspace counts under: its `org_id`, or (a row without one) its first owner's
/// personal org.
pub fn org_of_workspace(tx: &mut Tx, ws: &str) -> Result<String, DbError> {
    let row = tx.query_one(
        "SELECT w.org_id, COALESCE((SELECT MIN(m.user_id) FROM members m WHERE m.workspace_id = w.id AND m.role = 'owner'), '') FROM workspaces w WHERE w.id = $1",
        &[ws.into()],
    )?;
    Ok(match row {
        Some(r) if !r.text(0).is_empty() => r.text(0),
        Some(r) if !r.text(1).is_empty() => crate::orgs::personal_id(&r.text(1)),
        _ => String::new(),
    })
}

/// The 402 for a create beyond `max`.
pub fn refused(state: &State, tx: &mut Tx, e: &Entitlement, metric: Metric, used: i64, max: i64) -> ApiError {
    refused_named(state, tx, e, metric.name(), metric.what(), used, max)
}

/// [`refused`] for a limit outside [`Metric`] (run history and alerts, runs.rs): `limit` names it
/// `limit`, `what` in the message; `max` 0 = the plan doesn't have it.
pub fn refused_named(state: &State, tx: &mut Tx, e: &Entitlement, limit: &str, what: &str, used: i64, max: i64) -> ApiError {
    let (name, personal) = tx
        .query_one("SELECT name, personal_of FROM orgs WHERE id = $1", &[(&e.org).into()])
        .ok()
        .flatten()
        .map_or((String::new(), false), |r| (r.text(0), !r.text(1).is_empty()));
    let whose = if personal { "your personal account".to_string() } else { format!("the organization {name}") };
    let plan = e.plan_in_force().to_string();
    let url = format!("{}#/orgs/{}/plan", state.config.portal_url, e.org);
    ApiError {
        status: 402,
        code: "limit",
        message: if max == 0 {
            format!("plan limit: the {plan} plan of {whose} has no {what}; nothing was changed. See or change the plan at {url}")
        } else {
            format!("plan limit: the {plan} plan of {whose} allows {max} {what}; nothing was changed. See or change the plan at {url}")
        },
        extra: Some(json!({ "limit": limit, "plan": plan, "org": e.org, "used": used, "max": max, "upgradeUrl": url })),
    }
}

/// A feature only some plans have (HITOP Cloud C5: SSO, audit export, admin console): with
/// `HITOP_SYNC_LIMITS=plans`, refused with 402 `limit` = `feature` unless the plan in force is one
/// of `plans`. A self-hosted server has every feature.
pub fn require_plan(state: &State, tx: &mut Tx, org: &str, feature: &str, what: &str, plans: &[&str]) -> Result<(), ApiError> {
    if !state.config.plan_limits {
        return Ok(());
    }
    crate::billing::expire(state, tx, Some(org))?;
    let e = entitlement(tx, org)?;
    if plans.contains(&e.plan_in_force()) {
        return Ok(());
    }
    Err(refused_named(state, tx, &e, feature, what, 0, 0))
}

/// Refuses when `used_after` would exceed the org's limit for `metric`.
fn check(state: &State, tx: &mut Tx, org: &str, metric: Metric, used_after: impl FnOnce(&mut Tx, &Limits) -> Result<i64, DbError>) -> Result<(), ApiError> {
    if !state.config.plan_limits || org.is_empty() {
        return Ok(());
    }
    crate::billing::expire(state, tx, Some(org))?;
    let e = entitlement(tx, org)?;
    let limits = e.limits();
    let Some(max) = limits.get(metric) else { return Ok(()) };
    let after = used_after(tx, &limits)?;
    if after > max {
        return Err(refused(state, tx, &e, metric, after - 1, max));
    }
    Ok(())
}

/// One more workspace in `org`.
pub fn check_workspace(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    check(state, tx, org, Metric::Workspaces, |tx, _| Ok(tx.query_one("SELECT COUNT(*) FROM workspaces WHERE org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0)) + 1))
}

/// `growth` more stored bytes in `org` (only called when a write makes storage grow).
pub fn check_storage(state: &State, tx: &mut Tx, org: &str, growth: i64) -> Result<(), ApiError> {
    if growth <= 0 {
        return Ok(());
    }
    let r = check(state, tx, org, Metric::StorageBytes, |tx, _| {
        Ok(tx.query_one("SELECT CAST(COALESCE(SUM(bytes), 0) AS BIGINT) FROM workspaces WHERE org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0)) + growth)
    });
    // `used` in the error is what is stored now.
    r.map_err(|mut e| {
        if let Some(Json::Object(extra)) = &mut e.extra {
            let used = extra.get("used").and_then(Json::as_i64).unwrap_or(0) + 1 - growth;
            extra.insert("used".into(), used.into());
        }
        e
    })
}

/// One more runner credential in `org`.
pub fn check_runner(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    check(state, tx, org, Metric::Runners, |tx, _| {
        Ok(tx.query_one("SELECT COUNT(*) FROM users u JOIN workspaces w ON w.id = u.runner_of WHERE w.org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0)) + 1)
    })
}

/// One more invite today in `org`.
pub fn check_invite(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    let today = day(state);
    check(state, tx, org, Metric::InvitesPerDay, |tx, _| Ok(counter(tx, org, "invites", today)? + 1))
}

/// `person` (a user id, or `invite:<email>`) joins `org` (as an org member or a workspace role:
/// `editor` = anything but viewer).
pub fn check_seat(state: &State, tx: &mut Tx, org: &str, person: &str, editor: bool) -> Result<(), ApiError> {
    check(state, tx, org, Metric::Seats, |tx, limits| {
        let before = people(tx, org)?;
        let mut after = before.clone();
        after.add(person, editor);
        let (b, a) = (before.seats_used(limits.guest_viewers), after.seats_used(limits.guest_viewers));
        // Not a new seat: let it through whatever the count is.
        Ok(if a > b { a } else { i64::MIN })
    })
}

/// The device limit of a person: the highest of the orgs they belong to.
pub fn device_limit(tx: &mut Tx, user_id: &str) -> Result<Option<i64>, DbError> {
    let orgs: Vec<String> = tx.query("SELECT org_id FROM org_members WHERE user_id = $1", &[user_id.into()])?.iter().map(|r| r.text(0)).collect();
    let mut best = Some(0);
    for org in orgs {
        best = match (best, entitlement(tx, &org)?.limits().devices) {
            (_, None) | (None, _) => None,
            (Some(a), Some(b)) => Some(a.max(b)),
        };
    }
    Ok(best.map(|b| if b == 0 { plan_limits("free", 0).devices.unwrap_or(0) } else { b }))
}

/// One more device key for `user_id` (whose personal org exists).
pub fn check_device(state: &State, tx: &mut Tx, user_id: &str) -> Result<(), ApiError> {
    if !state.config.plan_limits {
        return Ok(());
    }
    let orgs: Vec<String> = tx.query("SELECT org_id FROM org_members WHERE user_id = $1", &[user_id.into()])?.iter().map(|r| r.text(0)).collect();
    for org in orgs {
        crate::billing::expire(state, tx, Some(&org))?;
    }
    let Some(max) = device_limit(tx, user_id)? else { return Ok(()) };
    let have = tx.query_one("SELECT COUNT(*) FROM device_keys WHERE user_id = $1", &[user_id.into()])?.map_or(0, |r| r.int(0));
    if have + 1 > max {
        let e = entitlement(tx, &crate::orgs::personal_id(user_id))?;
        return Err(refused(state, tx, &e, Metric::Devices, have, max));
    }
    Ok(())
}

/// Moving workspace `ws` into `org`: everything it brings must fit.
pub fn check_move(state: &State, tx: &mut Tx, org: &str, ws: &str) -> Result<(), ApiError> {
    if !state.config.plan_limits {
        return Ok(());
    }
    check_workspace(state, tx, org)?;
    let bytes = tx.query_one("SELECT bytes FROM workspaces WHERE id = $1", &[ws.into()])?.map_or(0, |r| r.int(0));
    check_storage(state, tx, org, bytes)?;
    let runners = tx.query_one("SELECT COUNT(*) FROM users WHERE runner_of = $1", &[ws.into()])?.map_or(0, |r| r.int(0));
    if runners > 0 {
        check(state, tx, org, Metric::Runners, |tx, _| {
            Ok(tx.query_one("SELECT COUNT(*) FROM users u JOIN workspaces w ON w.id = u.runner_of WHERE w.org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0)) + runners)
        })?;
    }
    let members = tx.query(
        "SELECT m.user_id, m.role FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND u.runner_of = ''",
        &[ws.into()],
    )?;
    check(state, tx, org, Metric::Seats, |tx, limits| {
        let before = people(tx, org)?;
        let mut after = before.clone();
        for m in &members {
            after.add(&m.text(0), m.text(1) != "viewer");
        }
        let (b, a) = (before.seats_used(limits.guest_viewers), after.seats_used(limits.guest_viewers));
        Ok(if a > b { a } else { i64::MIN })
    })
}

/// Migration 4: each personal org's limits become at least its usage (module docs).
pub fn grandfather(tx: &mut Tx) -> Result<(), DbError> {
    let orgs = tx.query("SELECT id, personal_of FROM orgs ORDER BY id", &[])?;
    for o in orgs {
        let (org, owner) = (o.text(0), o.text(1));
        let e = entitlement(tx, &org)?;
        let limits = plan_limits(&e.plan, e.seats);
        let p = people(tx, &org)?;
        let ws = tx.query_one("SELECT COUNT(*), CAST(COALESCE(SUM(bytes), 0) AS BIGINT) FROM workspaces WHERE org_id = $1", &[(&org).into()])?;
        let runners = tx.query_one("SELECT COUNT(*) FROM users u JOIN workspaces w ON w.id = u.runner_of WHERE w.org_id = $1", &[(&org).into()])?.map_or(0, |r| r.int(0));
        let devices = tx.query_one("SELECT COUNT(*) FROM device_keys WHERE user_id = $1", &[(&owner).into()])?.map_or(0, |r| r.int(0));
        let used = [
            (Metric::Seats, p.seats_used(limits.guest_viewers)),
            (Metric::Workspaces, ws.as_ref().map_or(0, |r| r.int(0))),
            (Metric::StorageBytes, ws.as_ref().map_or(0, |r| r.int(1))),
            (Metric::Runners, runners),
            (Metric::Devices, devices),
        ];
        let mut floors = e.overrides.clone();
        for (m, v) in used {
            if limits.get(m).is_some_and(|max| v > max) {
                floors.insert(m.name().into(), v.into());
            }
        }
        if floors != e.overrides {
            floors.insert("grandfathered".into(), true.into());
            tx.execute("UPDATE entitlements SET limits = $1 WHERE org_id = $2", &[Json::Object(floors).to_string().into(), (&org).into()])?;
        }
    }
    Ok(())
}

/// One limit someone is over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Over {
    pub org: String,
    pub metric: Metric,
    pub used: i64,
    pub max: i64,
}

/// Every org (and person, for devices) over a limit of its plan right now. After migration 4 this
/// is empty; later it lists orgs whose plan was lowered (their data stays; creates are refused).
pub fn over_limits(state: &State, tx: &mut Tx) -> Result<Vec<Over>, DbError> {
    crate::billing::expire(state, tx, None)?;
    let mut out = Vec::new();
    let orgs: Vec<String> = tx.query("SELECT id FROM orgs ORDER BY id", &[])?.iter().map(|r| r.text(0)).collect();
    for org in orgs {
        let limits = entitlement(tx, &org)?.limits();
        let u = usage(state, tx, &org)?;
        for m in [Metric::Seats, Metric::Workspaces, Metric::StorageBytes, Metric::Runners] {
            if let Some(max) = limits.get(m) {
                if u.get(m) > max {
                    out.push(Over { org: org.clone(), metric: m, used: u.get(m), max });
                }
            }
        }
    }
    let persons: Vec<(String, i64)> = tx
        .query("SELECT u.id, COUNT(k.id) FROM users u JOIN device_keys k ON k.user_id = u.id WHERE u.runner_of = '' GROUP BY u.id ORDER BY u.id", &[])?
        .iter()
        .map(|r| (r.text(0), r.int(1)))
        .collect();
    for (user, have) in persons {
        if let Some(max) = device_limit(tx, &user)? {
            if have > max {
                out.push(Over { org: crate::orgs::personal_id(&user), metric: Metric::Devices, used: have, max });
            }
        }
    }
    Ok(out)
}

/// Sets an org's plan and seats (`hitop-sync-server set-plan`, until billing writes them in C3).
pub fn set_plan(state: &State, org: &str, plan: &str, seats: i64) -> Result<(), String> {
    if !PLANS.contains(&plan) {
        return Err(format!("plan must be one of {}", PLANS.join(", ")));
    }
    state
        .db
        .tx(|tx| -> Result<(), DbError> {
            if tx.query_one("SELECT id FROM orgs WHERE id = $1", &[org.into()])?.is_none() {
                return Err(DbError(format!("no org {org}")));
            }
            let before = entitlement(tx, org)?;
            tx.execute(
                "INSERT INTO entitlements (org_id, plan, seats, limits, status, period_end, updated_at) VALUES ($1, $2, $3, '{}', 'active', 0, $4)
                 ON CONFLICT (org_id) DO UPDATE SET plan = excluded.plan, seats = excluded.seats, status = 'active', updated_at = excluded.updated_at",
                &[org.into(), plan.into(), seats.into(), state.now().into()],
            )?;
            tx.execute("UPDATE orgs SET plan = $1 WHERE id = $2", &[plan.into(), org.into()])?;
            let chain = crate::orgs::chain(org);
            crate::audit::record(
                tx,
                state,
                crate::audit::Event::new(&chain, "plan_changed", None).target(org).detail(json!({ "from": before.plan, "to": plan, "seats": seats, "by": "operator" })),
            )
        })
        .map_err(|e| e.to_string())
}

/// Drops usage counters older than 400 days (hourly with the audit purge).
pub fn purge(state: &State) -> Result<u64, DbError> {
    let cutoff = day(state) - 400;
    state.db.tx(|tx| tx.execute("DELETE FROM usage_counters WHERE day < $1", &[cutoff.into()]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_follow_section_3() {
        let free = plan_limits("free", 0);
        assert_eq!((free.seats, free.workspaces, free.storage_bytes, free.devices, free.runners), (Some(3), Some(3), Some(100 * MB), Some(3), Some(5)));
        let team = plan_limits("team", 4);
        assert_eq!((team.seats, team.guest_viewers, team.workspaces, team.storage_bytes, team.runners), (Some(4), 5, None, Some(14 * GB), Some(40)));
        let business = plan_limits("business", 10);
        assert_eq!((business.storage_bytes, business.runners), (Some(70 * GB), Some(250)));
        assert_eq!(plan_limits("enterprise", 0).seats, None);
        assert_eq!(plan_limits("unknown", 0), free, "anything unknown is Free");
    }

    #[test]
    fn overrides_raise_limits_and_lapsed_plans_are_free() {
        let mut e = Entitlement { org: "o".into(), plan: "free".into(), seats: 0, overrides: Map::new(), status: "active".into(), period_end: 0 };
        e.overrides.insert("workspaces".into(), 7.into());
        e.overrides.insert("seats".into(), 2.into());
        let l = e.limits();
        assert_eq!((l.workspaces, l.seats), (Some(7), Some(3)), "a floor only raises");
        e.plan = "enterprise".into();
        assert_eq!(e.limits().workspaces, Some(7), "on Enterprise an override is the contract");
        e.plan = "pro".into();
        e.status = "expired".into();
        assert_eq!(e.limits().workspaces, Some(7));
        assert_eq!(e.limits().devices, Some(3), "an expired Pro is Free");
    }

    #[test]
    fn guests_use_seats_beyond_the_free_ones() {
        let mut p = People::default();
        p.add("a", true);
        for g in ["g1", "g2", "g3"] {
            p.add(g, false);
        }
        assert_eq!(p.seats_used(0), 4);
        assert_eq!(p.seats_used(5), 1);
        p.add("g1", true);
        assert_eq!((p.core.len(), p.guests.len()), (2, 2), "an editor is a seat");
        p.add("a", false);
        assert!(p.core.contains("a"), "a viewer role elsewhere doesn't make a seat a guest");
    }
}
