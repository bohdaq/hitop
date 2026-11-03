//! Run history (HITOP Cloud C4): `hitop run --sync-report` and `hitop sync report <file>` upload a
//! run as a plaintext status row (passed or failed, timing, request counts, the collection and
//! environment names, failed request names only when the workspace's owners allow it) and its
//! full report.json sealed under the workspace key (hitop-sync-crypto `seal_run`); `hitop sync
//! runs` lists the history and opens reports on this device; `hitop sync alerts` sets where status
//! changes are announced (owners).

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use hitop_core::report::Report;
use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

use super::client::{b64, unb64, Workspace};

/// Seconds since 1970 of an RFC 3339 UTC time (`2026-11-03T10:00:00Z`, with or without fractions),
/// or `None`.
fn unix_time(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let time = time.trim_end_matches('Z');
    let time = time.split(['+', '.']).next()?;
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next().flatten().unwrap_or(0));
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400 + hh * 3600 + mm * 60 + ss)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Uploads one run of the folder's workspace (the signed-in member, or HITOP_SYNC_RUNNER).
/// Returns what the server kept.
pub fn upload(dir: &Path, report: &Report) -> Result<String> {
    let t = super::target(dir)?;
    let policy = t.api.call("GET", &format!("/v1/workspaces/{}/run-policy", t.workspace), None)?;
    let id = crypto::random_hex::<16>();
    let mut body = json!({ "id": id, "startedAt": unix_time(&report.started_at).unwrap_or_else(now) });
    let collection = match report.collections.as_slice() {
        [c] => c.name.clone(),
        [] => String::new(),
        many => format!("{} collections", many.len()),
    };
    body["collection"] = json!(collection);
    body["environment"] = json!(report.collections.first().and_then(|c| c.environment.clone()).unwrap_or_default());
    if policy["statusRows"].as_bool().unwrap_or(true) {
        body["status"] = json!(if report.passed { "passed" } else { "failed" });
        body["durationMs"] = json!(report.duration_ms);
        body["requests"] = json!(report.summary.requests.total);
        body["failed"] = json!(report.summary.requests.failed);
        if policy["failedNames"].as_bool().unwrap_or(false) {
            let mut names: Vec<String> = report.collections.iter().flat_map(|c| &c.iterations).flat_map(|i| &i.requests).filter(|r| !r.passed).map(|r| r.name.clone()).collect();
            names.dedup();
            body["failedNames"] = json!(names);
        }
    }
    let sealed = policy["sealedHistory"].as_bool().unwrap_or(true);
    if sealed {
        let ws = Workspace::open(&t.api, &t.device, &t.workspace)?;
        // The report as `--reporter json` writes it: secrets are already ••••.
        let text = report.to_json();
        let max = policy["maxRunBytes"].as_u64().unwrap_or(1 << 20) as usize;
        let detail = crypto::seal_run(ws.current_key(), &t.workspace, &id, ws.epoch, text.as_bytes()).map_err(|e| anyhow!("{e}"))?;
        if detail.len() > max {
            eprintln!("{} the report is {} bytes, more than the server keeps ({max}); uploading the status only", "note:".yellow(), detail.len());
        } else {
            body["detail"] = json!(b64(&detail));
        }
    }
    if body.get("status").is_none() && body.get("detail").is_none() {
        bail!("this workspace keeps no status rows and this plan no sealed reports: nothing to upload");
    }
    let r = t.api.call("POST", &format!("/v1/workspaces/{}/runs", t.workspace), Some(body))?;
    if let Some(n) = r["note"].as_str() {
        eprintln!("{} {n}", "note:".yellow());
    } else if !sealed {
        eprintln!("{} this plan keeps status rows only (no sealed report)", "note:".yellow());
    }
    Ok(r["stored"].as_str().unwrap_or("").to_string())
}

/// `hitop run --sync-report`: never changes the run's exit code; a failed upload is a warning.
pub fn upload_after_run(dir: &Path, report: &Report) {
    match upload(dir, report) {
        Ok(stored) => eprintln!("{} run uploaded to the workspace's history ({stored})", "Sync:".dimmed()),
        Err(e) => eprintln!("{} the run was not uploaded: {e:#}", "warning:".yellow()),
    }
}

/// `hitop sync report <report.json>`: uploads a report a run wrote (CI after the Action).
pub fn report_file(file: &Path, dir: &Path) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| file.display().to_string())?;
    let report: Report = serde_json::from_str(&text).with_context(|| format!("{} is not a hitop report.json", file.display()))?;
    let stored = upload(dir, &report)?;
    println!("Uploaded the run ({}) to the workspace's history: {stored}.", if report.passed { "passed" } else { "failed" });
    Ok(())
}

fn when(secs: i64) -> String {
    // UTC, minutes: 2026-11-03 10:04.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, rem % 3600 / 60)
}

/// `hitop sync runs`: the workspace's history (status rows), or one run's report opened here.
pub fn list(dir: &Path, json_out: bool, id: Option<&str>, limit: usize) -> Result<()> {
    let s = super::session()?;
    let link = super::store::link_of(dir)?.ok_or_else(|| anyhow!("{} isn't synced (`hitop sync link`)", dir.display()))?;
    let ws_id = link.workspace;
    if let Some(id) = id {
        let r = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/runs/{id}"), None)?;
        let Some(detail) = r["detail"].as_str() else {
            if json_out {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!("{} has a status row only (no sealed report): {} {}", id, r["status"].as_str().unwrap_or(""), r["collection"].as_str().unwrap_or(""));
            }
            return Ok(());
        };
        let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
        let sealed = unb64(detail)?;
        let epoch = crypto::run_epoch(&sealed).map_err(|e| anyhow!("{e}"))?;
        let key = ws.keys.get(&epoch).ok_or_else(|| anyhow!("the report is under key epoch {epoch}, which this device doesn't have"))?;
        let text = crypto::open_run(key, &ws_id, id, &sealed).map_err(|e| anyhow!("the report of run {id} doesn't open: {e} (refusing it)"))?;
        let report: Report = serde_json::from_slice(&text).context("the opened report is not a report.json")?;
        // The status row is the server's: it must say what the sealed report says.
        let row_status = r["status"].as_str().unwrap_or("");
        if row_status != "sealed" && row_status != if report.passed { "passed" } else { "failed" } {
            eprintln!("{} the server's status row says {row_status}, the sealed report says otherwise: the report is what ran", "warning:".red().bold());
        }
        if json_out {
            println!("{}", String::from_utf8_lossy(&text));
            return Ok(());
        }
        println!("Run {id}: {} at {}, {} ms", if report.passed { "passed".green() } else { "failed".red() }, report.started_at, report.duration_ms);
        for c in &report.collections {
            println!("{} {}", c.name.bold(), c.environment.as_deref().map(|e| format!("({e})")).unwrap_or_default());
            for it in &c.iterations {
                for req in &it.requests {
                    let mark = if req.passed { "✓".green() } else { "✗".red() };
                    let status = req.status.map(|s| s.to_string()).unwrap_or_else(|| "-".into());
                    println!("  {mark} {} {} {status} ({} ms){}", req.method.dimmed(), req.name, req.duration_ms, req.failure.as_deref().or(req.error.as_deref()).map(|f| format!(" {}", f.red())).unwrap_or_default());
                }
            }
        }
        println!("{}/{} requests passed", report.summary.requests.passed, report.summary.requests.total);
        return Ok(());
    }
    let r = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/runs?limit={}", limit.clamp(1, 500)), None)?;
    if json_out {
        println!("{}", serde_json::to_string_pretty(&r)?);
        return Ok(());
    }
    let runs = r["runs"].as_array().cloned().unwrap_or_default();
    if runs.is_empty() {
        println!("No runs yet. Upload them with `hitop run <folder> --sync-report` or `hitop sync report <report.json>`.");
        return Ok(());
    }
    for run in &runs {
        let status = match run["status"].as_str().unwrap_or("") {
            "passed" => "passed".green(),
            "failed" => "FAILED".red().bold(),
            other => other.normal(),
        };
        let env = run["environment"].as_str().filter(|e| !e.is_empty()).map(|e| format!(" ({e})")).unwrap_or_default();
        let counts = if run["status"] == "sealed" { String::new() } else { format!("  {}/{} failed, {} ms", run["failed"], run["requests"], run["durationMs"]) };
        let names = run["failedNames"].as_array().filter(|n| !n.is_empty()).map(|n| format!("  [{}]", n.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "))).unwrap_or_default();
        println!(
            "{}  {:<8} {}{env}{counts}{names}  {}{}",
            when(run["createdAt"].as_i64().unwrap_or(0)),
            status,
            run["collection"].as_str().unwrap_or(""),
            run["id"].as_str().unwrap_or("").dimmed(),
            if run["hasReport"].as_bool().unwrap_or(false) { "" } else { " (status only)" }
        );
    }
    if r["more"].as_bool().unwrap_or(false) {
        println!("(more: --limit {})", (limit * 2).min(500));
    }
    println!("History is kept {} days here. Open a report: hitop sync runs --id <id>", r["historyDays"]);
    Ok(())
}

/// What `hitop sync alerts` changes; `None` keeps the setting.
#[derive(Default)]
pub struct AlertChanges {
    pub email: Vec<String>,
    pub no_email: bool,
    pub webhook: Vec<String>,
    pub slack: Vec<String>,
    pub no_webhooks: bool,
    pub failed_names: Option<bool>,
    pub status_rows: Option<bool>,
    pub missed_after: Option<i64>,
    pub log: bool,
}

/// `hitop sync alerts` (owners): shows the settings, or changes them.
pub fn alerts(dir: &Path, c: AlertChanges) -> Result<()> {
    let (s, ws_id, _) = super::team(dir)?;
    if c.log {
        let r = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/alert-deliveries"), None)?;
        for d in r["deliveries"].as_array().cloned().unwrap_or_default() {
            println!("{}  {:<9} {:<10} {:<9} {}{}", when(d["createdAt"].as_i64().unwrap_or(0)), d["event"].as_str().unwrap_or(""), d["channel"].as_str().unwrap_or("").split(':').next().unwrap_or(""), d["outcome"].as_str().unwrap_or(""), d["collection"].as_str().unwrap_or(""), d["environment"].as_str().filter(|e| !e.is_empty()).map(|e| format!(" ({e})")).unwrap_or_default());
        }
        return Ok(());
    }
    let current = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/alerts"), None)?;
    let mut settings = current["settings"].clone();
    let changed = c.no_email || !c.email.is_empty() || !c.webhook.is_empty() || !c.slack.is_empty() || c.no_webhooks || c.failed_names.is_some() || c.status_rows.is_some() || c.missed_after.is_some();
    if changed {
        if c.no_email {
            settings["email"] = json!([]);
        }
        let mut emails: Vec<Value> = settings["email"].as_array().cloned().unwrap_or_default();
        emails.extend(c.email.iter().map(|e| json!(e)));
        settings["email"] = Value::Array(emails);
        if c.no_webhooks {
            settings["webhooks"] = json!([]);
        }
        let mut hooks: Vec<Value> = settings["webhooks"].as_array().cloned().unwrap_or_default();
        hooks.extend(c.webhook.iter().map(|u| json!({ "kind": "webhook", "url": u })));
        hooks.extend(c.slack.iter().map(|u| json!({ "kind": "slack", "url": u })));
        settings["webhooks"] = Value::Array(hooks);
        if let Some(v) = c.failed_names {
            settings["failedNames"] = json!(v);
        }
        if let Some(v) = c.status_rows {
            settings["statusRows"] = json!(v);
        }
        if let Some(v) = c.missed_after {
            settings["missedAfterMinutes"] = json!(v);
        }
        let r = s.api.call("PUT", &format!("/v1/workspaces/{ws_id}/alerts"), Some(settings))?;
        settings = r["settings"].clone();
        println!("Alert settings saved.");
    }
    let list = |v: &Value| v.as_array().map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "none".into());
    println!("Mail to: {}", list(&settings["email"]));
    for (i, w) in settings["webhooks"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        println!("{} {}: {}", w["kind"].as_str().unwrap_or(""), i + 1, w["url"].as_str().unwrap_or(""));
    }
    println!("Status rows: {} (off = history only, no alerts)", if settings["statusRows"].as_bool().unwrap_or(true) { "on" } else { "off" });
    println!("Failed request names in status rows and alerts: {}", if settings["failedNames"].as_bool().unwrap_or(false) { "yes" } else { "no" });
    match settings["missedAfterMinutes"].as_i64().unwrap_or(0) {
        0 => println!("Missed-run alerts: off"),
        m => println!("Missed-run alert after {m} minutes without a run"),
    }
    if let Some(ch) = current["plan"]["alertChannels"].as_array() {
        println!("This plan's channels: {}", ch.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_time("2026-11-03T10:04:05.123Z"), Some(1_793_700_245));
        assert_eq!(when(1_793_700_245), "2026-11-03 10:04");
        assert_eq!(unix_time("nonsense"), None);
    }
}
