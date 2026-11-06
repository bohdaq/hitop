//! Monitors (HITOP Cloud, owner decision 2026-11-06): the server runs a library collection on a
//! schedule and mails when it starts failing and when it passes again. Paid plans when plans are
//! on. Shared (end to end encrypted) collections can't be run here: the server can't read them.
//!
//!   GET/PUT/DELETE /v1/library/collection/:id/monitor   {environment, everyMinutes, emails}
//!   POST           /v1/library/collection/:id/monitor/run   (run at the next worker pass)
//!
//! Isolation: the server never runs a collection itself. The worker ([`run_due`], every 15 s)
//! starts `<bin> monitor-run` under `ulimit -v` with the collection, its environment and globals
//! (secret values opened under the host key) on stdin, and kills it after `RUN_FOR`. The child
//! ([`child_main`]) refuses private and listed addresses (`monitor_guard`, also for IP literals
//! and redirects), limits script loops and recursion, sends at most `MAX_REQUESTS` requests and
//! prints a summary: names, statuses, failed test names and errors, with secret values masked.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::IpAddr;
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::api::{body, random_hex, ApiError, ApiResult, State};
use crate::auth::authenticate;
use crate::db::Tx;
use crate::mail::Mail;

pub const EVERY: [i64; 6] = [5, 15, 30, 60, 360, 1440];
const MAX_EMAILS: usize = 5;
const MAX_REQUESTS: usize = 50;
const KEEP_RUNS: i64 = 100;
const RUN_FOR: Duration = Duration::from_secs(150);
const MEMORY_KB: u64 = 2 * 1024 * 1024;

// --- API ----------------------------------------------------------------------------------

fn collection_of(params: &PathParams) -> Result<String, ApiError> {
    let id = params.get("id").unwrap_or_default().to_string();
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::bad("a collection id"));
    }
    Ok(id)
}

fn monitor_json(tx: &mut Tx, r: &crate::db::Row) -> Result<Json, ApiError> {
    let id = r.text(0);
    let runs = tx.query("SELECT at, passed, requests, failed, duration_ms, failures FROM monitor_runs WHERE monitor_id = $1 ORDER BY at DESC LIMIT 20", &[(&id).into()])?;
    Ok(json!({
        "environment": r.text(1), "everyMinutes": r.int(2), "emails": serde_json::from_str::<Json>(&r.text(3)).unwrap_or(json!([])),
        "nextAt": r.int(4), "lastStatus": r.text(5), "lastRunAt": r.int(6),
        "runs": runs.iter().map(|x| json!({ "at": x.int(0), "passed": x.int(1) != 0, "requests": x.int(2), "failed": x.int(3), "durationMs": x.int(4), "failures": serde_json::from_str::<Json>(&x.text(5)).unwrap_or(json!([])) })).collect::<Vec<_>>()
    }))
}

const COLS: &str = "id, environment, every_minutes, emails, next_at, last_status, last_run_at";

pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_of(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let r = tx
            .query_one(&format!("SELECT {COLS} FROM monitors WHERE user_id = $1 AND collection_id = $2"), &[(&user.id).into(), (&collection).into()])?
            .ok_or_else(|| ApiError::not_found("no monitor for this collection"))?;
        Ok((200, monitor_json(tx, &r)?))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Put {
    #[serde(default)]
    environment: String,
    every_minutes: i64,
    #[serde(default)]
    emails: Option<Vec<String>>,
}

pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_of(params)?;
    let input: Put = body(request)?;
    if !EVERY.contains(&input.every_minutes) {
        return Err(ApiError::bad(format!("everyMinutes is one of {EVERY:?}")));
    }
    if state.config.monitor_bin.is_none() {
        return Err(ApiError::new(404, "monitors_off", "this server doesn't run monitors"));
    }
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let org = tx
            .query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2 AND deleted = 0", &[(&user.id).into(), (&collection).into()])?
            .map(|r| r.text(0))
            .ok_or_else(|| ApiError::not_found("no such collection in your HITOP Cloud library"))?;
        crate::mocks::plan_allows_feature(state, tx, &org, "monitors")?;
        let emails = input.emails.unwrap_or_else(|| vec![user.email.clone()]);
        if emails.len() > MAX_EMAILS || emails.iter().any(|e| !e.contains('@') || e.len() > 254 || e.contains(char::is_whitespace)) {
            return Err(ApiError::bad(format!("emails: up to {MAX_EMAILS} addresses")));
        }
        let emails = serde_json::to_string(&emails).unwrap_or_default();
        let env = input.environment.trim().to_string();
        let now = state.now();
        let existing = tx.query_one("SELECT id FROM monitors WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&collection).into()])?;
        if existing.is_some() {
            tx.execute(
                "UPDATE monitors SET environment = $1, every_minutes = $2, emails = $3, next_at = $4 WHERE user_id = $5 AND collection_id = $6",
                &[(&env).into(), input.every_minutes.into(), (&emails).into(), now.into(), (&user.id).into(), (&collection).into()],
            )?;
        } else {
            tx.execute(
                "INSERT INTO monitors (id, user_id, collection_id, environment, every_minutes, emails, next_at, last_status, last_run_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, '', 0, $7)",
                &[random_hex::<12>().into(), (&user.id).into(), (&collection).into(), (&env).into(), input.every_minutes.into(), (&emails).into(), now.into()],
            )?;
        }
        let r = tx.query_one(&format!("SELECT {COLS} FROM monitors WHERE user_id = $1 AND collection_id = $2"), &[(&user.id).into(), (&collection).into()])?.expect("just written");
        Ok((200, monitor_json(tx, &r)?))
    })
}

pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_of(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        if let Some(r) = tx.query_one("SELECT id FROM monitors WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&collection).into()])? {
            tx.execute("DELETE FROM monitor_runs WHERE monitor_id = $1", &[r.text(0).into()])?;
            tx.execute("DELETE FROM monitors WHERE id = $1", &[r.text(0).into()])?;
        }
        Ok((200, json!({ "deleted": true })))
    })
}

/// `POST .../monitor/run`: due now (at most once a minute).
pub fn run_now(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_of(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let r = tx
            .query_one("SELECT last_run_at FROM monitors WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&collection).into()])?
            .ok_or_else(|| ApiError::not_found("no monitor for this collection"))?;
        let now = state.now();
        if now - r.int(0) < 60 {
            return Err(ApiError::new(429, "rate_limited", "a monitor runs at most once a minute"));
        }
        tx.execute("UPDATE monitors SET next_at = $1 WHERE user_id = $2 AND collection_id = $3", &[now.into(), (&user.id).into(), (&collection).into()])?;
        Ok((202, json!({ "queued": true })))
    })
}

// --- the worker ---------------------------------------------------------------------------

/// What the child gets on stdin.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Input {
    pub collection: Json,
    pub environment: Option<Json>,
    pub secrets: HashMap<String, String>,
    pub globals: HashMap<String, String>,
    pub global_secrets: HashMap<String, String>,
    pub deny: Vec<String>,
    pub allow_private: bool,
}

/// What the child prints.
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub passed: bool,
    pub requests: i64,
    pub failed: i64,
    pub duration_ms: i64,
    pub failures: Vec<Json>,
}

fn failed(message: &str) -> Outcome {
    Outcome { passed: false, failures: vec![json!({ "name": "monitor", "error": message })], ..Default::default() }
}

/// The run's input from the library: None when the collection is gone.
fn prepare(state: &State, tx: &mut Tx, user: &str, collection: &str, env_name: &str) -> Result<Option<Input>, ApiError> {
    let Some((data, _)) = crate::library::item_with_secrets(state, tx, user, "collection", collection)? else { return Ok(None) };
    let mut input = Input { collection: data, deny: state.config.deny_ips.clone(), allow_private: state.config.monitor_allow_private, ..Default::default() };
    if !env_name.is_empty() {
        if let Some((envs, secrets)) = crate::library::item_with_secrets(state, tx, user, "environments", "all")? {
            if let Some(env) = envs["environments"].as_array().into_iter().flatten().find(|e| e["name"].as_str() == Some(env_name)) {
                let id = env["id"].as_str().unwrap_or_default();
                if let Some(m) = secrets[id].as_object() {
                    input.secrets = m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
                }
                input.environment = Some(env.clone());
            }
        }
    }
    if let Some((g, secrets)) = crate::library::item_with_secrets(state, tx, user, "globals", "all")? {
        input.globals = g["variables"].as_object().map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
        if let Some(m) = secrets.as_object() {
            input.global_secrets = m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
        }
    }
    Ok(Some(input))
}

/// Runs `input` in a child process: its summary, or a failure saying why there is none.
pub fn run_child(bin: &std::path::Path, input: &Input) -> Outcome {
    let script = format!("ulimit -v {MEMORY_KB} 2>/dev/null; exec \"$0\" monitor-run");
    let child = Command::new("sh").arg("-c").arg(script).arg(bin).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).env_clear().env("PATH", "/usr/bin:/bin").spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return failed(&format!("the monitor couldn't start: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&serde_json::to_vec(input).unwrap_or_default());
    }
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + RUN_FOR;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(&format!("the run took longer than {} s", RUN_FOR.as_secs()));
            }
        }
    }
    let out = reader.join().unwrap_or_default();
    serde_json::from_slice(&out).unwrap_or_else(|_| failed("the run stopped without a result (too much memory, or a crash)"))
}

/// Runs the monitors that are due; how many ran.
pub fn run_due(state: &State) -> usize {
    let Some(bin) = state.config.monitor_bin.clone() else { return 0 };
    let now = state.now();
    // Take the due ones and move them on first, so another pass doesn't take them too.
    let due = state.db.tx(|tx| {
        let rows = tx.query("SELECT id, user_id, collection_id, environment, every_minutes, emails, last_status FROM monitors WHERE next_at <= $1 ORDER BY next_at LIMIT 4", &[now.into()])?;
        let mut out = Vec::new();
        for r in rows {
            tx.execute("UPDATE monitors SET next_at = $1 WHERE id = $2", &[(now + r.int(4) * 60).into(), r.text(0).into()])?;
            out.push((r.text(0), r.text(1), r.text(2), r.text(3), r.text(5), r.text(6)));
        }
        Ok::<_, ApiError>(out)
    });
    let Ok(due) = due else { return 0 };
    for (id, user, collection, env, emails, last) in &due {
        let prepared = state.db.tx(|tx| {
            let org = tx.query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2", &[user.into(), collection.into()])?.map(|r| r.text(0)).unwrap_or_default();
            if crate::mocks::plan_allows_feature(state, tx, &org, "monitors").is_err() {
                return Ok::<_, ApiError>(Err("this collection's plan no longer includes monitors"));
            }
            Ok(prepare(state, tx, user, collection, env)?.ok_or("the collection is no longer in the library"))
        });
        let outcome = match prepared {
            Ok(Ok(input)) => run_child(&bin, &input),
            Ok(Err(why)) => failed(why),
            Err(e) => failed(&e.message),
        };
        record(state, id, collection, user, emails, last, &outcome);
    }
    due.len()
}

fn record(state: &State, id: &str, collection: &str, user: &str, emails: &str, last: &str, o: &Outcome) {
    let now = state.now();
    let status = if o.passed { "passed" } else { "failed" };
    let name = state
        .db
        .tx(|tx| {
            let mut at = now;
            while tx.query_one("SELECT at FROM monitor_runs WHERE monitor_id = $1 AND at = $2", &[id.into(), at.into()])?.is_some() {
                at += 1;
            }
            tx.execute(
                "INSERT INTO monitor_runs (monitor_id, at, passed, requests, failed, duration_ms, failures) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[id.into(), at.into(), i64::from(o.passed).into(), o.requests.into(), o.failed.into(), o.duration_ms.into(), Json::Array(o.failures.clone()).to_string().into()],
            )?;
            tx.execute("DELETE FROM monitor_runs WHERE monitor_id = $1 AND at <= (SELECT at FROM monitor_runs WHERE monitor_id = $1 ORDER BY at DESC LIMIT 1 OFFSET $2)", &[id.into(), KEEP_RUNS.into()])?;
            tx.execute("UPDATE monitors SET last_status = $1, last_run_at = $2 WHERE id = $3", &[status.into(), now.into(), id.into()])?;
            let name = crate::library::collection_item(tx, user, collection)?.map(|(d, _)| d["name"].as_str().unwrap_or("A collection").to_string()).unwrap_or_else(|| "A collection".into());
            Ok::<_, ApiError>(name)
        })
        .unwrap_or_default();
    // Mail on a change: it starts failing, or it passes again.
    let changed = (status == "failed" && last != "failed") || (status == "passed" && last == "failed");
    if !changed || !state.mailer.enabled() {
        return;
    }
    let subject = if o.passed { format!("HITOP monitor: {name} passes again") } else { format!("HITOP monitor: {name} is failing") };
    let mut body = if o.passed {
        format!("{name} passed all {} requests again.\n", o.requests)
    } else {
        format!("{name}: {} of {} requests failed.\n\n", o.failed, o.requests)
    };
    for f in o.failures.iter().take(10) {
        let detail = [f["status"].as_u64().map(|s| format!("status {s}")), f["error"].as_str().map(String::from), f["tests"].as_array().filter(|t| !t.is_empty()).map(|t| format!("failed: {}", t.iter().filter_map(Json::as_str).collect::<Vec<_>>().join(", ")))];
        body.push_str(&format!("- {}: {}\n", f["name"].as_str().unwrap_or("?"), detail.into_iter().flatten().collect::<Vec<_>>().join("; ")));
    }
    body.push_str(&format!("\nThe monitor and its runs: open the collection in HITOP, Share, Monitor.\n{}\n", state.config.public_url));
    for to in serde_json::from_str::<Vec<String>>(emails).unwrap_or_default() {
        state.mailer.send(Mail { to, subject: subject.clone(), body: body.clone() });
    }
}

// --- the child ----------------------------------------------------------------------------

static GUARD: OnceLock<(Vec<IpAddr>, bool)> = OnceLock::new();

fn url_allowed(url: &str) -> Result<(), String> {
    let (deny, allow_private) = GUARD.get().cloned().unwrap_or_default();
    let u = reqwest::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    if u.scheme() != "http" && u.scheme() != "https" {
        return Err("a monitor sends only http and https requests".into());
    }
    if allow_private {
        return Ok(());
    }
    let port = u.port_or_known_default().unwrap_or(0);
    if !crate::monitor_guard::port_allowed(port) {
        return Err(format!("port {port} is blocked"));
    }
    let literal = match u.host() {
        Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
        _ => None,
    };
    if let Some(ip) = literal {
        if !crate::monitor_guard::ip_allowed(ip, &deny) {
            return Err(format!("{ip} is a private or reserved address: a monitor reaches only public addresses"));
        }
    }
    Ok(())
}

/// `hitop-sync-server monitor-run`: reads an [`Input`] on stdin, prints an [`Outcome`].
pub fn child_main() -> i32 {
    let mut raw = Vec::new();
    if std::io::stdin().read_to_end(&mut raw).is_err() {
        return 2;
    }
    let input: Input = match serde_json::from_slice(&raw) {
        Ok(i) => i,
        Err(e) => {
            println!("{}", serde_json::to_string(&failed(&format!("bad input: {e}"))).unwrap_or_default());
            return 0;
        }
    };
    let deny: Vec<IpAddr> = input.deny.iter().filter_map(|s| s.parse().ok()).collect();
    let _ = GUARD.set((deny.clone(), input.allow_private));
    hitop_core::http::install_url_check(url_allowed);
    let redirect_check = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if let Err(e) = url_allowed(attempt.url().as_str()) {
            attempt.error(format!("redirect refused: {e}"))
        } else {
            attempt.follow()
        }
    });
    let mut builder = reqwest::blocking::Client::builder().use_rustls_tls().no_proxy().redirect(redirect_check).connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(30));
    if !input.allow_private {
        builder = builder.dns_resolver(Arc::new(crate::monitor_guard::PublicResolver { deny }));
    }
    match builder.build() {
        Ok(c) => {
            hitop_core::http::install_client(c);
        }
        Err(e) => {
            println!("{}", serde_json::to_string(&failed(&format!("HTTP client: {e}"))).unwrap_or_default());
            return 0;
        }
    }
    hitop_core::scripting::set_limits(1_000_000, 200);
    println!("{}", serde_json::to_string(&run(input)).unwrap_or_default());
    0
}

fn run(input: Input) -> Outcome {
    use hitop_core::model::{Collection, Environment};
    use hitop_core::secrets::{Origin, Secrets};
    let mut collection: Collection = match serde_json::from_value(input.collection) {
        Ok(c) => c,
        Err(e) => return failed(&format!("the collection can't be read: {e}")),
    };
    // WebSocket and gRPC requests aren't run (as in `hitop run`), and at most MAX_REQUESTS.
    collection.requests.retain(|r| !r.is_websocket() && !r.is_grpc());
    collection.requests.truncate(MAX_REQUESTS);
    let environment: Option<Environment> = input.environment.and_then(|e| serde_json::from_value(e).ok());
    let mut secrets = Secrets::default();
    for (k, v) in &input.secrets {
        secrets.insert(k, v.clone(), Origin::Flag);
    }
    let mut global_secrets = Secrets::default();
    let mut globals = input.globals;
    for (k, v) in &input.global_secrets {
        global_secrets.insert(k, v.clone(), Origin::Flag);
        globals.insert(k.clone(), v.clone());
    }
    let options = hitop_core::runner::RunOptions { stop_on_fail: false, timeout: Some(Duration::from_secs(30)), overrides: HashMap::new(), globals, global_secrets };
    let started = Instant::now();
    let steps = hitop_core::runner::run_collection_opts(&collection, environment.as_ref(), &secrets, &options, &mut |_| {});
    let failures: Vec<Json> = steps
        .iter()
        .filter(|s| !s.passed())
        .map(|s| {
            let (status, error) = match &s.result {
                Ok(r) => (Some(r.status), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            json!({ "name": s.request_name, "status": status, "error": error, "tests": s.tests.iter().filter(|t| !t.pass).map(|t| t.name.clone()).collect::<Vec<_>>() })
        })
        .collect();
    Outcome { passed: failures.is_empty(), requests: steps.len() as i64, failed: failures.len() as i64, duration_ms: started.elapsed().as_millis() as i64, failures }
}
