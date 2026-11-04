//! Alerts from run status rows (HITOP Cloud C4): mail, a JSON webhook, or a Slack incoming
//! webhook, when a series (collection + environment) of a workspace changes status (passed →
//! failed, failed → passed; a first run that fails), and once when a series misses its expected
//! run. Never a response body, a request, a header or a secret value: only what the status row
//! has (runs.rs), and failed request names only when the workspace's owners allow it.
//!
//! The upload only queues alerts (`alert_deliveries`, outcome `queued`), in its transaction; a
//! worker thread delivers them ([`deliver_pending`], every few seconds) so a slow webhook never
//! holds an upload, and [`tick`] (hourly) finds missed runs. Each delivery checks the settings
//! again: an address that is no longer a member's gets nothing, a webhook that was removed isn't
//! called. A workspace sends at most `HITOP_SYNC_ALERTS_PER_HOUR` (30) alerts an hour; the rest are
//! logged as `suppressed`.
//!
//! Webhook URLs are secrets (a Slack URL posts to a channel): only owners read or change them,
//! they're never in the audit log, and the server calls only `https://` URLs whose host resolves to
//! public addresses (no redirects, 10 s), so an alert can't reach the server's own network.

use std::net::{IpAddr, ToSocketAddrs};

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::api::{body, random_hex, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::authenticate;
use crate::db::{DbError, Tx};
use crate::workspaces::{membership, require_role, Role};

const MAX_EMAILS: usize = 20;
const MAX_WEBHOOKS: usize = 5;
const MAX_ATTEMPTS: i64 = 3;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Webhook {
    /// `webhook` (JSON, section 4) or `slack` (an incoming webhook: `{"text"}`).
    pub kind: String,
    pub url: String,
}

/// A workspace's alert settings (owners set them).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Settings {
    /// Addresses of members that get alert mail.
    #[serde(default)]
    pub email: Vec<String>,
    #[serde(default)]
    pub webhooks: Vec<Webhook>,
    /// Whether uploads may carry the names of failed requests (status rows, webhook bodies).
    #[serde(default)]
    pub failed_names: bool,
    /// `false`: "history only, no alerts": uploads carry no status row.
    #[serde(default = "yes")]
    pub status_rows: bool,
    /// A series with no run for this many minutes gets one "missed run" alert (0 = off).
    #[serde(default)]
    pub missed_after_minutes: i64,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings { email: vec![], webhooks: vec![], failed_names: false, status_rows: true, missed_after_minutes: 0 }
    }
}

pub fn settings(tx: &mut Tx, ws: &str) -> Result<Settings, DbError> {
    Ok(tx.query_one("SELECT settings FROM alert_settings WHERE workspace_id = $1", &[ws.into()])?.and_then(|r| serde_json::from_str(&r.text(0)).ok()).unwrap_or_default())
}

/// Person members' addresses (lower case).
fn member_emails(tx: &mut Tx, ws: &str) -> Result<Vec<String>, DbError> {
    Ok(tx
        .query("SELECT u.email FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND u.runner_of = ''", &[ws.into()])?
        .iter()
        .map(|r| r.text(0).to_lowercase())
        .collect())
}

/// Whether `url` may be called: https (or, with `insecure_local`, http to 127.0.0.1), and every
/// address its host resolves to is public. Checked when settings are saved and before each call.
pub fn check_url(url: &str, insecure_local: bool) -> Result<url::Url, String> {
    let u = url::Url::parse(url).map_err(|_| format!("{url} is not a URL"))?;
    let host = u.host_str().ok_or("the URL has no host")?.trim_matches(['[', ']']).to_string();
    if !u.username().is_empty() || u.password().is_some() {
        return Err("put no user or password in the URL".into());
    }
    let local_test = insecure_local && u.scheme() == "http" && host == "127.0.0.1";
    if local_test {
        return Ok(u);
    }
    if u.scheme() != "https" {
        return Err("webhooks must be https:// URLs".into());
    }
    let port = u.port_or_known_default().unwrap_or(443);
    let addrs: Vec<IpAddr> = (host.as_str(), port).to_socket_addrs().map_err(|e| format!("{host}: {e}"))?.map(|a| a.ip()).collect();
    if addrs.is_empty() {
        return Err(format!("{host} doesn't resolve"));
    }
    if let Some(a) = addrs.iter().find(|a| !public(a)) {
        return Err(format!("{host} resolves to {a}, which isn't a public address"));
    }
    Ok(u)
}

/// A public unicast address (not loopback, private, link-local, CGNAT, documentation,
/// multicast, unspecified, unique local).
pub fn public(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_loopback() || v.is_private() || v.is_link_local() || v.is_multicast() || v.is_broadcast() || v.is_unspecified() || v.is_documentation() || o[0] == 0 || (o[0] == 100 && (o[1] & 0xc0) == 64) || (o[0] == 192 && o[1] == 0 && o[2] == 0) || o[0] >= 240)
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return public(&IpAddr::V4(v4));
            }
            let s = v.segments();
            !(v.is_loopback() || v.is_unspecified() || v.is_multicast() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80 || s[0] == 0x2001 && s[1] == 0x0db8)
        }
    }
}

/// Which alert channels the plan allows: refuses a webhook (Pro and up) or Slack (Team and up)
/// beyond it.
fn check_channels(state: &State, tx: &mut Tx, ws: &str, s: &Settings) -> Result<(), ApiError> {
    let org = crate::limits::org_of_workspace(tx, ws)?;
    let (plan, ent) = crate::runs::run_plan(state, tx, &org)?;
    let Some(e) = ent else { return Ok(()) };
    if s.webhooks.iter().any(|w| w.kind == "webhook") && !plan.webhooks {
        return Err(crate::runs::refused(state, tx, &e, "alertWebhooks", "webhook alerts", 0, 0));
    }
    if s.webhooks.iter().any(|w| w.kind == "slack") && !plan.slack {
        return Err(crate::runs::refused(state, tx, &e, "alertSlack", "Slack alerts", 0, 0));
    }
    Ok(())
}

/// `GET /v1/workspaces/:ws/alerts` (owner): the settings, webhook URLs included.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Owner)?;
        let s = settings(tx, id)?;
        let org = crate::limits::org_of_workspace(tx, id)?;
        Ok((200, json!({ "settings": s, "plan": crate::runs::plan_json(state, tx, &org)? })))
    })
}

/// `PUT /v1/workspaces/:ws/alerts {email, webhooks, failedNames, statusRows, missedAfterMinutes}`
/// (owner): replaces the settings. Addresses must be members'; webhooks https to public hosts;
/// webhook and Slack channels need a plan that has them (402 `limit`).
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let mut s: Settings = body(request)?;
    s.email = s.email.iter().map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()).collect();
    s.email.sort();
    s.email.dedup();
    if s.email.len() > MAX_EMAILS || s.webhooks.len() > MAX_WEBHOOKS {
        return Err(ApiError::bad(format!("at most {MAX_EMAILS} addresses and {MAX_WEBHOOKS} webhooks")));
    }
    for w in &s.webhooks {
        if w.kind != "webhook" && w.kind != "slack" {
            return Err(ApiError::bad("a webhook's kind is webhook or slack"));
        }
        check_url(&w.url, state.config.runs.insecure_local_webhooks).map_err(|e| ApiError::bad(format!("webhook {}: {e}", w.url)))?;
    }
    if s.missed_after_minutes != 0 && !(5..=44_640).contains(&s.missed_after_minutes) {
        return Err(ApiError::bad("missedAfterMinutes is 0 (off) or 5 to 44640 (31 days)"));
    }
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Owner)?;
        let members = member_emails(tx, id)?;
        if let Some(e) = s.email.iter().find(|e| !members.contains(e)) {
            return Err(ApiError::bad(format!("{e} is not a member of this workspace: alert mail goes to members only")));
        }
        check_channels(state, tx, id, &s)?;
        tx.execute(
            "INSERT INTO alert_settings (workspace_id, settings, updated_by, updated_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (workspace_id) DO UPDATE SET settings = excluded.settings, updated_by = excluded.updated_by, updated_at = excluded.updated_at",
            &[id.into(), serde_json::to_string(&s).unwrap_or_default().into(), (&user.id).into(), state.now().into()],
        )?;
        // The log names the channels, never a webhook URL.
        let kinds: Vec<&str> = s.webhooks.iter().map(|w| w.kind.as_str()).collect();
        audit::record(
            tx,
            state,
            Event::new(id, "alerts_changed", Some(&user)).detail(json!({ "emails": s.email.len(), "webhooks": kinds, "failedNames": s.failed_names, "statusRows": s.status_rows, "missedAfterMinutes": s.missed_after_minutes })),
        )?;
        Ok((200, json!({ "settings": s })))
    })
}

/// `GET /v1/workspaces/:ws/alert-deliveries` (owner): the last 100 alerts and how they went.
pub fn deliveries(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Owner)?;
        let rows = tx.query(
            "SELECT id, event, channel, collection, environment, run_id, outcome, attempts, created_at FROM alert_deliveries WHERE workspace_id = $1 ORDER BY created_at DESC, id DESC LIMIT 100",
            &[id.into()],
        )?;
        let list: Vec<Json> = rows
            .iter()
            .map(|r| {
                // Webhooks by position (`webhook:1`), never their URL.
                json!({ "id": r.text(0), "event": r.text(1), "channel": r.text(2), "collection": r.text(3), "environment": r.text(4), "run": r.text(5), "outcome": r.text(6), "attempts": r.int(7), "createdAt": r.int(8) })
            })
            .collect();
        Ok((200, json!({ "deliveries": list })))
    })
}

/// A delivery id that sorts in the order alerts were queued (so a webhook gets "failed" before
/// "recovered"): nanoseconds since 1970, then random.
fn delivery_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{nanos:024x}{}", random_hex::<4>())
}

/// Queues one alert per channel, unless the hourly ceiling is reached (then one `suppressed`
/// row). Returns how many were queued.
#[allow(clippy::too_many_arguments)]
fn enqueue(state: &State, tx: &mut Tx, ws: &str, s: &Settings, event: &str, collection: &str, environment: &str, run: &str, names: &[String]) -> Result<usize, DbError> {
    let now = state.now();
    let mut channels: Vec<String> = s.email.iter().map(|e| format!("email:{e}")).collect();
    channels.extend(s.webhooks.iter().enumerate().map(|(i, w)| format!("{}:{i}", w.kind)));
    if channels.is_empty() {
        return Ok(0);
    }
    let sent_hour = tx
        .query_one("SELECT COUNT(DISTINCT event || ':' || collection || ':' || environment || ':' || run_id) FROM alert_deliveries WHERE workspace_id = $1 AND created_at > $2 AND outcome <> 'suppressed'", &[ws.into(), (now - 3600).into()])?
        .map_or(0, |r| r.int(0));
    let names = if s.failed_names { serde_json::to_string(names).unwrap_or_default() } else { String::new() };
    let insert = |tx: &mut Tx, channel: &str, outcome: &str| {
        tx.execute(
            "INSERT INTO alert_deliveries (id, workspace_id, event, channel, collection, environment, run_id, failed_names, outcome, attempts, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0, $10, $10)",
            &[delivery_id().into(), ws.into(), event.into(), channel.into(), collection.into(), environment.into(), run.into(), names.clone().into(), outcome.into(), now.into()],
        )
    };
    if sent_hour >= state.config.runs.alerts_per_hour {
        insert(tx, "all", "suppressed")?;
        return Ok(0);
    }
    for c in &channels {
        insert(tx, c, "queued")?;
    }
    Ok(channels.len())
}

/// After an upload with a status row: updates the series and queues an alert when its status
/// changed (or a first run failed). Returns the channels queued.
#[allow(clippy::too_many_arguments)]
pub fn on_run(state: &State, tx: &mut Tx, ws: &str, run: &str, collection: &str, environment: &str, status: &str, names: &[String]) -> Result<Vec<String>, DbError> {
    let previous = tx.query_one("SELECT last_status FROM alert_state WHERE workspace_id = $1 AND collection = $2 AND environment = $3", &[ws.into(), collection.into(), environment.into()])?.map(|r| r.text(0));
    tx.execute(
        "INSERT INTO alert_state (workspace_id, collection, environment, last_status, last_run_at, missed_alerted_at) VALUES ($1, $2, $3, $4, $5, 0)
         ON CONFLICT (workspace_id, collection, environment) DO UPDATE SET last_status = excluded.last_status, last_run_at = excluded.last_run_at, missed_alerted_at = 0",
        &[ws.into(), collection.into(), environment.into(), status.into(), state.now().into()],
    )?;
    let event = match (previous.as_deref(), status) {
        (None | Some("passed"), "failed") => "failed",
        (Some("failed"), "passed") => "recovered",
        _ => return Ok(vec![]),
    };
    let s = settings(tx, ws)?;
    let n = enqueue(state, tx, ws, &s, event, collection, environment, run, names)?;
    Ok(vec![event.to_string(); n])
}

/// Finds series that missed their run (no run for `missedAfterMinutes`) and queues one alert per
/// gap. Hourly from main.rs; tests call it after moving the clock. Returns the alerts queued.
pub fn tick(state: &State) -> Result<usize, DbError> {
    let now = state.now();
    state.db.tx(|tx| {
        let mut n = 0;
        for row in tx.query("SELECT workspace_id, settings FROM alert_settings", &[])? {
            let ws = row.text(0);
            let Ok(s) = serde_json::from_str::<Settings>(&row.text(1)) else { continue };
            if s.missed_after_minutes <= 0 || !s.status_rows {
                continue;
            }
            let cutoff = now - s.missed_after_minutes * 60;
            let late = tx.query(
                "SELECT collection, environment FROM alert_state WHERE workspace_id = $1 AND last_run_at < $2 AND missed_alerted_at < last_run_at + 1",
                &[(&ws).into(), cutoff.into()],
            )?;
            for l in late {
                let (c, e) = (l.text(0), l.text(1));
                tx.execute("UPDATE alert_state SET missed_alerted_at = $1 WHERE workspace_id = $2 AND collection = $3 AND environment = $4", &[now.into(), (&ws).into(), (&c).into(), (&e).into()])?;
                n += enqueue(state, tx, &ws, &s, "missed", &c, &e, "", &[])?;
            }
        }
        Ok(n)
    })
}

/// What an alert says, from its row: the JSON webhook body (section 4) and a line of text.
#[allow(clippy::too_many_arguments)]
fn message(state: &State, tx: &mut Tx, ws: &str, event: &str, collection: &str, environment: &str, run: &str, names: &[String]) -> Result<(Json, String), DbError> {
    let r = if run.is_empty() {
        None
    } else {
        tx.query_one("SELECT status, started_at, duration_ms, requests, failed FROM runs WHERE workspace_id = $1 AND id = $2", &[ws.into(), run.into()])?
    };
    let link = if run.is_empty() { format!("{}workspaces/{ws}/runs", state.config.portal_url) } else { format!("{}workspaces/{ws}/runs/{run}", state.config.portal_url) };
    let what = if collection.is_empty() { "a collection".to_string() } else { collection.to_string() };
    let env = if environment.is_empty() { String::new() } else { format!(" ({environment})") };
    let mut body = json!({
        "event": event,
        "workspace": ws,
        "collection": collection,
        "environment": environment,
        "link": link,
    });
    let text = match (event, &r) {
        ("missed", _) => format!("HITOP: {what}{env} missed its scheduled run. {link}"),
        (_, Some(r)) => {
            body["status"] = r.text(0).into();
            body["run"] = run.into();
            body["startedAt"] = r.int(1).into();
            body["durationMs"] = r.int(2).into();
            body["requests"] = r.int(3).into();
            body["failed"] = r.int(4).into();
            let verb = if event == "failed" { "failed" } else { "passes again" };
            format!("HITOP: {what}{env} {verb}: {} of {} requests failed. {link}", r.int(4), r.int(3))
        }
        _ => format!("HITOP: {what}{env} {event}. {link}"),
    };
    if !names.is_empty() {
        body["failedRequests"] = json!(names);
    }
    Ok((body, text))
}

/// Delivers queued alerts (the worker every few seconds; tests call it). Returns how many went
/// out. Failures are retried on later passes, three attempts in all.
pub fn deliver_pending(state: &State) -> Result<usize, DbError> {
    let rows = state.db.tx(|tx| {
        tx.query(
            "SELECT id, workspace_id, event, channel, collection, environment, run_id, failed_names, attempts FROM alert_deliveries WHERE outcome = 'queued' ORDER BY created_at, id LIMIT 100",
            &[],
        )
    })?;
    let mut sent = 0;
    for r in rows {
        let (id, ws, event, channel) = (r.text(0), r.text(1), r.text(2), r.text(3));
        let names: Vec<String> = serde_json::from_str(&r.text(7)).unwrap_or_default();
        // Settings and members as they are now.
        let prepared = state.db.tx(|tx| {
            let s = settings(tx, &ws)?;
            let names = if s.failed_names { names.clone() } else { vec![] };
            let (body, text) = message(state, tx, &ws, &event, &r.text(4), &r.text(5), &r.text(6), &names)?;
            Ok::<_, DbError>((s, member_emails(tx, &ws)?, body, text))
        })?;
        let (s, members, body, text) = prepared;
        let result: Result<(), String> = match channel.split_once(':') {
            Some(("email", addr)) if s.email.iter().any(|e| e == addr) && members.iter().any(|m| m == addr) => {
                if !state.mailer.enabled() {
                    Err("this server sends no mail".into())
                } else {
                    let subject = match event.as_str() {
                        "failed" => "A scheduled run failed",
                        "recovered" => "A scheduled run passes again",
                        _ => "A scheduled run is missing",
                    };
                    state.mailer.send(crate::mail::Mail { to: addr.to_string(), subject: format!("HITOP: {subject}"), body: format!("{text}\n\nAlerts for this workspace are set by its owners in HITOP (Sync page or the HITOP Cloud portal).") });
                    Ok(())
                }
            }
            Some(("email", _)) => Err("skipped: no longer an alert address of a member".into()),
            Some((kind @ ("webhook" | "slack"), i)) => match i.parse::<usize>().ok().and_then(|i| s.webhooks.get(i)).filter(|w| w.kind == kind) {
                Some(w) => {
                    let payload = if kind == "slack" { json!({ "text": text }) } else { body };
                    post(state, &w.url, &payload)
                }
                None => Err("skipped: the webhook was removed".into()),
            },
            _ => Err("skipped: unknown channel".into()),
        };
        let attempts = r.int(8) + 1;
        let outcome = match &result {
            Ok(()) => "sent",
            Err(e) if e.starts_with("skipped") => "skipped",
            Err(_) if attempts >= MAX_ATTEMPTS => "failed",
            Err(_) => "queued",
        };
        if let Err(e) = &result {
            // The channel, not the address or URL (personal data, secret).
            eprintln!("hitop-sync-server: alert {id} ({}): {e}", channel.split(':').next().unwrap_or(""));
        } else {
            sent += 1;
        }
        state.db.tx(|tx| tx.execute("UPDATE alert_deliveries SET outcome = $1, attempts = $2, updated_at = $3 WHERE id = $4", &[outcome.into(), attempts.into(), state.now().into(), (&id).into()]))?;
    }
    Ok(sent)
}

fn post(state: &State, url: &str, payload: &Json) -> Result<(), String> {
    let u = check_url(url, state.config.runs.insecure_local_webhooks)?;
    let agent = crate::oidc::http_agent();
    let res = agent.post(u.as_str()).header("Content-Type", "application/json").send(payload.to_string()).map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(format!("the webhook answered {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses() {
        for a in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0", "::1", "fe80::1", "fd00::1", "::ffff:127.0.0.1", "224.0.0.1", "192.0.0.8"] {
            assert!(!public(&a.parse().unwrap()), "{a}");
        }
        for a in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public(&a.parse().unwrap()), "{a}");
        }
    }

    #[test]
    fn webhook_urls() {
        assert!(check_url("http://example.com/x", false).is_err(), "http");
        assert!(check_url("https://127.0.0.1/x", false).is_err(), "loopback");
        assert!(check_url("https://user:pw@example.com/x", false).is_err(), "credentials");
        assert!(check_url("http://127.0.0.1:9/x", false).is_err());
        assert!(check_url("http://127.0.0.1:9/x", true).is_ok(), "the test setting");
        assert!(check_url("http://localhost:9/x", true).is_err(), "only the literal address");
    }
}
