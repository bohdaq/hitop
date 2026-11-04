//! Run history from the customer's own runners (HITOP Cloud C4, spec/CLOUD_SERVICE.md section 4
//! "Runs from your own runners").
//!
//! A CI job or a `hitop schedule` cron line runs a collection with its runner credential (or a
//! member's session) and uploads one row per run: a **plaintext status row** (pass/fail, timing,
//! request counts, the collection and environment names, and the names of failed requests only
//! when the workspace allows it), and the **sealed full report** (report.json v1, secrets masked)
//! that only members' devices can open (hitop-sync-crypto `seal_run`). The server can't read a
//! report; it keeps the rows for history, dashboards and alerts (alerts.rs).
//!
//! Runs are rows of their own, not workspace blobs: they never show in the change feed, so clients
//! before C4 sync exactly as before, and the retention purge doesn't touch version vectors. The
//! sealed bytes count towards the workspace's bytes (and so the org's storage limit).
//!
//! Plans (`HITOP_SYNC_LIMITS=plans`): run uploads a day per org (an abuse ceiling), how long rows
//! are kept, and whether the sealed report is kept at all (Free keeps status rows only). A
//! self-hosted server has no plans: no ceiling, `HITOP_SYNC_RUN_RETENTION_DAYS` (365) for all.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{b64, body, query, unb64, ApiError, ApiResult, State};
use crate::auth::{authenticate, User};
use crate::db::{DbError, Tx};
use crate::limits::{self, Entitlement};
use crate::workspaces::{membership, require_role, Role, Workspace};

/// Settings from the environment.
#[derive(Clone, Debug)]
pub struct RunsConfig {
    /// `HITOP_SYNC_MAX_RUN_BYTES`: the largest sealed report (default 1 MiB).
    pub max_run_bytes: usize,
    /// `HITOP_SYNC_RUN_RETENTION_DAYS`: how long rows are kept without plans (default 365; 0 keeps
    /// them forever). With plans, the plan says.
    pub retention_days: i64,
    /// `HITOP_SYNC_ALERTS_PER_HOUR`: alerts a workspace may send an hour (default 30), on top of
    /// "one per status change".
    pub alerts_per_hour: i64,
    /// `HITOP_SYNC_ALERT_WEBHOOKS=insecure-local`: webhooks may also be `http://127.0.0.1` (tests
    /// only; production webhooks are https to public addresses).
    pub insecure_local_webhooks: bool,
}

impl Default for RunsConfig {
    fn default() -> Self {
        RunsConfig { max_run_bytes: 1 << 20, retention_days: 365, alerts_per_hour: 30, insecure_local_webhooks: false }
    }
}

impl RunsConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<RunsConfig, String> {
        let mut c = RunsConfig::default();
        let number = |name: &str, default: i64| -> Result<i64, String> { var(name).map_or(Ok(default), |v| v.parse().map_err(|_| format!("{name} must be a number"))) };
        c.max_run_bytes = number("HITOP_SYNC_MAX_RUN_BYTES", c.max_run_bytes as i64)?.max(1024) as usize;
        c.retention_days = number("HITOP_SYNC_RUN_RETENTION_DAYS", c.retention_days)?.max(0);
        c.alerts_per_hour = number("HITOP_SYNC_ALERTS_PER_HOUR", c.alerts_per_hour)?.max(0);
        c.insecure_local_webhooks = match var("HITOP_SYNC_ALERT_WEBHOOKS").as_deref() {
            None | Some("https") => false,
            Some("insecure-local") => true,
            Some(other) => return Err(format!("HITOP_SYNC_ALERT_WEBHOOKS must be https or insecure-local, not {other}")),
        };
        Ok(c)
    }
}

/// What a plan allows for runs and alerts (section 3; a proposal until the owner sets prices).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    /// Run uploads a day per org; `None` = no ceiling.
    pub runs_per_day: Option<i64>,
    /// Days rows are kept; 0 = forever.
    pub history_days: i64,
    /// Whether the sealed report is kept (Free: status rows only).
    pub sealed_history: bool,
    pub webhooks: bool,
    pub slack: bool,
    /// Shared environment values (shares.rs).
    pub shared_secrets: bool,
}

pub fn plan_of(plan: &str) -> RunPlan {
    let p = |runs: Option<i64>, days: i64, sealed: bool, webhooks: bool, slack: bool| RunPlan { runs_per_day: runs, history_days: days, sealed_history: sealed, webhooks, slack, shared_secrets: sealed };
    match plan {
        "pro" => p(Some(1_000), 30, true, true, false),
        "team" => p(Some(5_000), 90, true, true, true),
        "business" => p(Some(20_000), 365, true, true, true),
        "enterprise" => p(None, 365, true, true, true),
        _ => p(Some(100), 7, false, false, false),
    }
}

/// The run plan in force for `org` (everything allowed, the configured retention, without plans).
/// Enterprise overrides `runsPerDay` and `historyDays` come from the entitlement.
pub fn run_plan(state: &State, tx: &mut Tx, org: &str) -> Result<(RunPlan, Option<Entitlement>), DbError> {
    if !state.config.plan_limits || org.is_empty() {
        return Ok((RunPlan { runs_per_day: None, history_days: state.config.runs.retention_days, sealed_history: true, webhooks: true, slack: true, shared_secrets: true }, None));
    }
    let e = limits::entitlement(tx, org)?;
    let mut p = plan_of(e.plan_in_force());
    if e.plan_in_force() == "enterprise" {
        if let Some(v) = e.overrides.get("runsPerDay").and_then(Json::as_i64) {
            p.runs_per_day = Some(v);
        }
        if let Some(v) = e.overrides.get("historyDays").and_then(Json::as_i64) {
            p.history_days = v.max(0);
        }
    }
    Ok((p, Some(e)))
}

/// The `runs` part of `GET /v1/orgs/:org/limits`.
pub fn plan_json(state: &State, tx: &mut Tx, org: &str) -> Result<Json, DbError> {
    let (p, _) = run_plan(state, tx, org)?;
    let mut channels = vec!["email"];
    if p.webhooks {
        channels.push("webhook");
    }
    if p.slack {
        channels.push("slack");
    }
    Ok(json!({ "runsPerDay": p.runs_per_day, "historyDays": p.history_days, "sealedHistory": p.sealed_history, "alertChannels": channels, "sharedSecrets": p.shared_secrets }))
}

/// Run uploads today in `org` (for `GET /v1/orgs/:org/usage`).
pub fn runs_today(state: &State, tx: &mut Tx, org: &str) -> Result<i64, DbError> {
    Ok(tx.query_one("SELECT count FROM usage_counters WHERE org_id = $1 AND metric = 'runs' AND day = $2", &[org.into(), limits::day(state).into()])?.map_or(0, |r| r.int(0)))
}

/// The 402 for a feature the plan doesn't have (`max` 0) or a ceiling (`max` the ceiling).
pub fn refused(state: &State, tx: &mut Tx, e: &Entitlement, name: &'static str, what: &str, used: i64, max: i64) -> ApiError {
    limits::refused_named(state, tx, e, name, what, used, max)
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}

fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

/// The workspace's alert settings that uploaders need (alerts.rs keeps the rest).
pub fn policy(tx: &mut Tx, ws: &str) -> Result<(bool, bool), DbError> {
    let s = crate::alerts::settings(tx, ws)?;
    Ok((s.status_rows, s.failed_names))
}

/// `GET /v1/workspaces/:ws/run-policy` (any member, runner credentials too): what an upload
/// should carry: `statusRows` (false = "history only, no alerts": send no status), `failedNames`
/// (whether failed request names may go in the status row), `sealedHistory` (whether the server
/// keeps the sealed report on this plan), `maxRunBytes`.
pub fn get_policy(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        membership(tx, id, &user)?;
        let (status_rows, failed_names) = policy(tx, id)?;
        let org = limits::org_of_workspace(tx, id)?;
        let (plan, _) = run_plan(state, tx, &org)?;
        Ok((200, json!({ "statusRows": status_rows, "failedNames": failed_names, "sealedHistory": plan.sealed_history, "historyDays": plan.history_days, "maxRunBytes": state.config.runs.max_run_bytes })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Upload {
    id: String,
    /// Unix seconds.
    started_at: i64,
    #[serde(default)]
    collection: String,
    #[serde(default)]
    environment: String,
    /// `passed` or `failed`; left out when the workspace keeps no status rows.
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    duration_ms: i64,
    #[serde(default)]
    requests: i64,
    #[serde(default)]
    failed: i64,
    #[serde(default)]
    failed_names: Vec<String>,
    /// The sealed report (base64url), `seal_run` under the current epoch.
    #[serde(default)]
    detail: Option<String>,
}

/// Who may upload: runner credentials of the workspace, editors and owners (viewers read).
fn may_upload(user: &User, ws: &Workspace) -> Result<(), ApiError> {
    if !user.runner_of.is_empty() {
        return Ok(());
    }
    require_role(ws, Role::Editor)
}

/// `POST /v1/workspaces/:ws/runs {id, startedAt, collection, environment, status, durationMs,
/// requests, failed, failedNames, detail}` (runner credential, editor or owner) → 201 `{id,
/// stored: "status+report"|"status"|"report", note?}`. The only write a runner credential may
/// make. 409 `exists` for a repeated id.
pub fn upload(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: Upload = body(request)?;
    if !valid_run_id(&input.id) {
        return Err(ApiError::bad("run ids are 1 to 64 letters, digits, '-' or '_' (random, from the client)"));
    }
    let status = match input.status.as_deref() {
        None => None,
        Some(s @ ("passed" | "failed")) => Some(s.to_string()),
        Some(_) => return Err(ApiError::bad("status must be passed or failed")),
    };
    if input.duration_ms < 0 || input.requests < 0 || input.failed < 0 || input.failed > input.requests {
        return Err(ApiError::bad("durationMs, requests and failed are counts (failed at most requests)"));
    }
    let detail = match &input.detail {
        None => Vec::new(),
        Some(d) => {
            let bytes = unb64("detail", d)?;
            if bytes.len() > state.config.runs.max_run_bytes {
                return Err(ApiError::new(413, "too_large", format!("a sealed report is at most {} bytes", state.config.runs.max_run_bytes)));
            }
            // Shape only: format 1 envelope (hitop-sync-crypto `seal_run`).
            if bytes.len() < 1 + 4 + 24 + 16 || bytes[0] != 1 {
                return Err(ApiError::bad("detail is not a sealed run report"));
            }
            bytes
        }
    };
    if status.is_none() && detail.is_empty() {
        return Err(ApiError::bad("a run needs a status, a sealed report, or both"));
    }
    let collection = clean(&input.collection, 200);
    let environment = clean(&input.environment, 100);
    let now = state.now();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        may_upload(&user, &ws)?;
        if !detail.is_empty() {
            let epoch = i64::from(u32::from_be_bytes([detail[1], detail[2], detail[3], detail[4]]));
            if epoch < 1 || epoch > ws.epoch {
                return Err(ApiError::conflict("stale_epoch", format!("seal the report with the current key epoch {}", ws.epoch), json!({ "epoch": ws.epoch })));
            }
        }
        if tx.query_one("SELECT id FROM runs WHERE workspace_id = $1 AND id = $2", &[id.into(), (&input.id).into()])?.is_some() {
            return Err(ApiError::conflict("exists", "a run with this id was uploaded already", json!({})));
        }
        let org = limits::org_of_workspace(tx, id)?;
        let (plan, ent) = run_plan(state, tx, &org)?;
        if let (Some(max), Some(e)) = (plan.runs_per_day, &ent) {
            let today = runs_today(state, tx, &org)?;
            if today + 1 > max {
                return Err(refused(state, tx, e, "runsPerDay", "run uploads a day", today, max));
            }
        }
        let (status_rows, failed_names) = policy(tx, id)?;
        let mut note = None;
        // Free keeps status rows only; a workspace with status rows off keeps reports only.
        let detail = if detail.is_empty() || plan.sealed_history {
            detail.clone()
        } else {
            note = Some(format!("the {} plan keeps status rows only: the sealed report was not stored (run history needs Pro or above, {}orgs/{org}/plan)", ent.as_ref().map_or("free", |e| e.plan_in_force()), state.config.portal_url));
            Vec::new()
        };
        let status = if status_rows { status.clone() } else { None };
        if status.is_none() && detail.is_empty() {
            return Err(ApiError::bad("nothing to store: this workspace keeps no status rows and this plan keeps no sealed reports"));
        }
        let names = if status.is_some() && failed_names { input.failed_names.iter().take(50).map(|n| clean(n, 200)).filter(|n| !n.is_empty()).collect::<Vec<_>>() } else { Vec::new() };
        let size = detail.len() as i64;
        if size > 0 {
            let bytes = ws.bytes + size;
            if bytes > state.config.max_workspace_bytes {
                return Err(ApiError::new(413, "quota", format!("the workspace would exceed {} bytes (run history counts)", state.config.max_workspace_bytes)));
            }
            limits::check_storage(state, tx, &org, size)?;
        }
        let (srow, counts) = match &status {
            Some(s) => (s.clone(), (input.duration_ms, input.requests, input.failed)),
            None => ("sealed".to_string(), (0, 0, 0)),
        };
        tx.execute(
            "INSERT INTO runs (workspace_id, id, uploader, collection, environment, status, started_at, duration_ms, requests, failed, failed_names, detail, detail_bytes, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
            &[
                id.into(),
                (&input.id).into(),
                (&user.id).into(),
                (&collection).into(),
                (&environment).into(),
                srow.into(),
                input.started_at.into(),
                counts.0.into(),
                counts.1.into(),
                counts.2.into(),
                if names.is_empty() { String::new() } else { serde_json::to_string(&names).unwrap_or_default() }.into(),
                detail.clone().into(),
                size.into(),
                now.into(),
            ],
        )?;
        if size > 0 {
            tx.execute("UPDATE workspaces SET bytes = bytes + $1 WHERE id = $2", &[size.into(), id.into()])?;
        }
        limits::meter(state, tx, &org, "runs", 1)?;
        let alerts = match &status {
            Some(s) => crate::alerts::on_run(state, tx, id, &input.id, &collection, &environment, s, &names)?,
            None => vec![],
        };
        let stored = match (status.is_some(), size > 0) {
            (true, true) => "status+report",
            (true, false) => "status",
            _ => "report",
        };
        let mut out = json!({ "id": input.id, "stored": stored, "alerts": alerts.len() });
        if let Some(n) = note {
            out["note"] = n.into();
        }
        Ok((201, out))
    })
}

fn row_json(r: &crate::db::Row, with_detail: bool) -> Json {
    let names: Vec<String> = serde_json::from_str(&r.text(10)).unwrap_or_default();
    let mut v = json!({
        "id": r.text(0),
        "uploader": r.text(1),
        "runner": r.int(13) != 0,
        "collection": r.text(2),
        "environment": r.text(3),
        "status": r.text(4),
        "startedAt": r.int(5),
        "durationMs": r.int(6),
        "requests": r.int(7),
        "failed": r.int(8),
        "failedNames": names,
        "createdAt": r.int(9),
        "hasReport": r.int(12) > 0,
        "reportBytes": r.int(12),
    });
    if with_detail && r.int(12) > 0 {
        v["detail"] = b64(&r.bytes(11)).into();
    }
    v
}

const COLUMNS: &str = "r.id, r.uploader, r.collection, r.environment, r.status, r.started_at, r.duration_ms, r.requests, r.failed, r.created_at, r.failed_names, r.detail, r.detail_bytes, CASE WHEN EXISTS (SELECT 1 FROM users u WHERE u.id = r.uploader AND u.runner_of <> '') THEN 1 ELSE 0 END";
const COLUMNS_NO_DETAIL: &str = "r.id, r.uploader, r.collection, r.environment, r.status, r.started_at, r.duration_ms, r.requests, r.failed, r.created_at, r.failed_names, r.detail_bytes, r.detail_bytes, CASE WHEN EXISTS (SELECT 1 FROM users u WHERE u.id = r.uploader AND u.runner_of <> '') THEN 1 ELSE 0 END";

/// `GET /v1/workspaces/:ws/runs?collection=&environment=&before=<createdAt>&limit=` (members, not
/// runner credentials): status rows, newest first (at most 500, default 100), and per series
/// (collection + environment) its last status. Sealed reports are fetched one at a time.
pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let q = query(request);
    let limit: i64 = q.get("limit").map_or(Ok(100), |s| s.parse()).map_err(|_| ApiError::bad("limit must be a number"))?.clamp(1, 500);
    let before: i64 = q.get("before").map_or(Ok(i64::MAX), |s| s.parse()).map_err(|_| ApiError::bad("before must be a number"))?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        membership(tx, id, &user)?;
        let mut sql = format!("SELECT {COLUMNS_NO_DETAIL} FROM runs r WHERE r.workspace_id = $1 AND r.created_at < $2");
        let mut args: Vec<crate::db::Value> = vec![id.into(), before.into()];
        for (key, col) in [("collection", "collection"), ("environment", "environment")] {
            if let Some(v) = q.get(key) {
                args.push(v.as_str().into());
                sql.push_str(&format!(" AND r.{col} = ${}", args.len()));
            }
        }
        args.push((limit + 1).into());
        sql.push_str(&format!(" ORDER BY r.created_at DESC, r.id DESC LIMIT ${}", args.len()));
        let rows = tx.query(&sql, &args)?;
        let more = rows.len() as i64 > limit;
        let runs: Vec<Json> = rows.iter().take(limit as usize).map(|r| row_json(r, false)).collect();
        let series: Vec<Json> = tx
            .query("SELECT collection, environment, last_status, last_run_at FROM alert_state WHERE workspace_id = $1 ORDER BY collection, environment", &[id.into()])?
            .iter()
            .map(|r| json!({ "collection": r.text(0), "environment": r.text(1), "status": r.text(2), "lastRunAt": r.int(3) }))
            .collect();
        let org = limits::org_of_workspace(tx, id)?;
        let (plan, _) = run_plan(state, tx, &org)?;
        Ok((200, json!({ "runs": runs, "more": more, "series": series, "historyDays": plan.history_days, "sealedHistory": plan.sealed_history })))
    })
}

/// `GET /v1/workspaces/:ws/runs/:id` (members): one status row with its sealed report
/// (`detail`, base64url) when it has one.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let run = params.get("id").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        membership(tx, id, &user)?;
        let row = tx.query_one(&format!("SELECT {COLUMNS} FROM runs r WHERE r.workspace_id = $1 AND r.id = $2"), &[id.into(), run.into()])?;
        let row = row.ok_or_else(|| ApiError::not_found("no such run"))?;
        Ok((200, row_json(&row, true)))
    })
}

/// Deletes rows past their plan's retention (hourly) and frees their bytes; returns how many went.
pub fn purge(state: &State) -> Result<u64, DbError> {
    let now = state.now();
    let workspaces: Vec<String> = state.db.tx(|tx| Ok::<_, DbError>(tx.query("SELECT DISTINCT workspace_id FROM runs", &[])?.iter().map(|r| r.text(0)).collect()))?;
    let mut n = 0;
    for ws in workspaces {
        n += state.db.tx(|tx| {
            let org = limits::org_of_workspace(tx, &ws)?;
            let (plan, _) = run_plan(state, tx, &org)?;
            if plan.history_days <= 0 {
                return Ok::<_, DbError>(0);
            }
            let cutoff = now - plan.history_days * 86_400;
            let freed = tx.query_one("SELECT CAST(COALESCE(SUM(detail_bytes), 0) AS BIGINT) FROM runs WHERE workspace_id = $1 AND created_at < $2", &[(&ws).into(), cutoff.into()])?.map_or(0, |r| r.int(0));
            let gone = tx.execute("DELETE FROM runs WHERE workspace_id = $1 AND created_at < $2", &[(&ws).into(), cutoff.into()])?;
            if freed > 0 {
                tx.execute("UPDATE workspaces SET bytes = CASE WHEN bytes > $1 THEN bytes - $1 ELSE 0 END WHERE id = $2", &[freed.into(), (&ws).into()])?;
            }
            Ok(gone)
        })?;
    }
    // Delivered alerts are kept 30 days (the owner's delivery log).
    state.db.tx(|tx| tx.execute("DELETE FROM alert_deliveries WHERE created_at < $1 AND outcome <> 'queued'", &[(now - 30 * 86_400).into()]))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_follow_section_3() {
        let free = plan_of("free");
        assert_eq!((free.runs_per_day, free.history_days, free.sealed_history, free.webhooks, free.shared_secrets), (Some(100), 7, false, false, false));
        let pro = plan_of("pro");
        assert_eq!((pro.runs_per_day, pro.history_days, pro.webhooks, pro.slack), (Some(1000), 30, true, false));
        assert_eq!((plan_of("team").runs_per_day, plan_of("team").history_days, plan_of("team").slack), (Some(5000), 90, true));
        assert_eq!((plan_of("business").runs_per_day, plan_of("business").history_days), (Some(20000), 365));
        assert_eq!(plan_of("nonsense"), free);
    }

    #[test]
    fn run_ids() {
        assert!(valid_run_id("2026-11-03T10-00-00Z-ab12"));
        assert!(!valid_run_id("runs/x") && !valid_run_id("") && !valid_run_id(&"a".repeat(65)));
    }
}
