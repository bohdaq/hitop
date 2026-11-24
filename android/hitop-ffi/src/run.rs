//! Collection runs: `runStart` runs a collection on its own thread the way `hitop run` does
//! (hitop-core `runner::run_step` per request, one `ReportBuilder` across iterations, WebSocket and
//! gRPC requests left out, `bail` stops at the first failure); `runNext {id, waitMs}` hands over
//! the steps finished since (masked) and, at the end, the reports (report.json = spec/run-report.md,
//! JUnit XML, HTML); `runCancel {id}` stops after the request in flight.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hitop_core::cookies::Jar;
use hitop_core::report::ReportBuilder;
use hitop_core::runner::{is_run_step, run_step, RunState};
use serde_json::{json, Value};

use crate::send::{inputs, masker, step_json};
use crate::Args;

struct Run {
    events: Receiver<Value>,
    cancel: Arc<AtomicBool>,
}

fn runs() -> &'static Mutex<HashMap<u64, Run>> {
    static RUNS: OnceLock<Mutex<HashMap<u64, Run>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn start(a: &Args) -> Result<Value, String> {
    let mut i = inputs(a)?;
    let rows: Vec<Option<crate::data::Row>> = match rows_of(a)? {
        Some(r) => r.into_iter().map(Some).collect(),
        None => vec![],
    };
    let files = crate::files::map(a)?;
    let iterations = a.0.get("iterations").and_then(Value::as_u64).unwrap_or(1).clamp(1, 100) as usize;
    // A data file: one iteration per row, the row's values as variables (as `hitop run --data`).
    let rows = if rows.is_empty() { vec![None; iterations] } else { rows };
    let iterations = rows.len();
    let bail = a.0.get("bail").and_then(Value::as_bool).unwrap_or(false);
    let tool = a.0.get("toolVersion").and_then(Value::as_str).unwrap_or(env!("CARGO_PKG_VERSION")).to_string();
    let mut jar: Option<Jar> = match a.0.get("jar") {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| format!("jar: {e}"))?),
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    runs().lock().unwrap_or_else(|p| p.into_inner()).insert(id, Run { events: rx, cancel: cancel.clone() });
    std::thread::spawn(move || {
        let _files = crate::files::Scope::new(files);
        // A run never opens a browser: it uses a token from an earlier send, or fails saying so.
        let _oauth = crate::oauth::Scope::new(None);
        let m = masker(&i);
        let steps_per_iteration = i.collection.requests.iter().filter(|r| is_run_step(r)).count();
        let mut builder = ReportBuilder::new(&tool);
        builder.collection(&i.collection.name, i.environment.as_ref().map(|e| e.name.as_str()));
        let mut bailed = false;
        let mut cancelled = false;
        'iterations: for (it, row) in rows.into_iter().enumerate() {
            i.options.overrides = row.iter().flatten().map(|(k, v)| (k.clone(), v.clone())).collect();
            builder.start_iteration();
            // A fresh state per iteration, as `hitop run` (run_collection_jar per row); cookies carry over.
            let mut state = RunState::new(&i.collection, i.environment.as_ref(), &i.secrets, &i.options);
            let mut steps = vec![];
            for saved in i.collection.requests.iter().filter(|r| is_run_step(r)) {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                let mut step = run_step(&i.collection, saved, &mut state, &i.options, jar.as_mut());
                step.mask(&m);
                let _ = tx.send(json!({ "type": "step", "iteration": it + 1, "index": steps.len(), "total": steps_per_iteration * iterations, "step": step_json(&step) }));
                let failed = !step.passed();
                steps.push(step);
                if failed && bail {
                    builder.iteration(row, &steps);
                    bailed = true;
                    break 'iterations;
                }
            }
            builder.iteration(row, &steps);
            if cancelled {
                break;
            }
        }
        let report = builder.finish(bailed);
        let _ = tx.send(json!({
            "type": "done",
            "cancelled": cancelled,
            "passed": report.passed,
            "reportJson": report.to_json(),
            "junit": report.to_junit(),
            "html": report.to_html(),
            "jar": jar.map(|j| serde_json::to_value(j).unwrap_or_default()),
        }));
    });
    Ok(json!({ "id": id }))
}

/// The rows of `data` (a CSV or JSON file's text) named `dataName`.
fn rows_of(a: &Args) -> Result<Option<Vec<crate::data::Row>>, String> {
    let Some(text) = a.opt_json::<String>("data")? else { return Ok(None) };
    let name: String = a.opt_json("dataName")?.unwrap_or_default();
    let rows = crate::data::parse(&text, std::path::Path::new(&name)).map_err(|e| format!("{e:#}"))?;
    match rows.len() {
        0 => Err("The data file has no rows.".into()),
        n if n > 1000 => Err(format!("The data file has {n} rows; at most 1000 run on a phone.")),
        _ => Ok(Some(rows)),
    }
}

/// How many iterations a data file makes, and its columns.
pub(crate) fn data_rows(a: &Args) -> Result<Value, String> {
    let rows = rows_of(a)?.unwrap_or_default();
    let columns: std::collections::BTreeSet<&String> = rows.iter().flat_map(|r| r.keys()).collect();
    Ok(json!({ "rows": rows.len(), "columns": columns }))
}

pub(crate) fn next(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    let wait = Duration::from_millis(a.0.get("waitMs").and_then(Value::as_u64).unwrap_or(500));
    let mut events = vec![];
    let mut done = false;
    {
        let map = runs().lock().unwrap_or_else(|p| p.into_inner());
        let run = map.get(&id).ok_or("no such run")?;
        // The first event may take a while (a request); then whatever else is ready.
        if let Ok(e) = run.events.recv_timeout(wait) {
            done |= e["type"] == "done";
            events.push(e);
            while let Ok(e) = run.events.try_recv() {
                done |= e["type"] == "done";
                events.push(e);
            }
        }
    }
    if done {
        runs().lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
    Ok(json!({ "events": events, "done": done }))
}

pub(crate) fn cancel(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    if let Some(r) = runs().lock().unwrap_or_else(|p| p.into_inner()).get(&id) {
        r.cancel.store(true, Ordering::Relaxed);
    }
    Ok(Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::{json, Value};

    fn call(op: &str, a: Value) -> Value {
        serde_json::from_str(&call_json(op, &a.to_string())).unwrap()
    }

    #[test]
    fn a_data_file_is_one_iteration_per_row() {
        assert_eq!(call("dataRows", json!({ "data": "user,n\nada,1\nbob,2\n", "dataName": "d.csv" }))["ok"], json!({ "rows": 2, "columns": ["n", "user"] }));
        assert!(call("dataRows", json!({ "data": "a,b\n1\n", "dataName": "d.csv" }))["error"].as_str().unwrap().contains("line 2"));
        let c = json!({ "id": 1, "name": "c", "requests": [{ "id": 2, "name": "r", "method": "GET", "url": "http://127.0.0.1:1/${user}", "headers": [], "body": "" }] });
        let id = call("runStart", json!({ "collection": c.to_string(), "data": "[{\"user\":\"ada\"},{\"user\":\"bob\"}]", "dataName": "d" }))["ok"]["id"].clone();
        let mut urls = vec![];
        loop {
            let r = call("runNext", json!({ "id": id, "waitMs": 2000 }));
            for e in r["ok"]["events"].as_array().unwrap() {
                if e["type"] == "step" {
                    urls.push(e["step"]["url"].as_str().unwrap().to_string());
                    assert_eq!(e["total"], 2);
                } else {
                    assert!(e["reportJson"].as_str().unwrap().contains("\"user\": \"bob\""), "{e}");
                }
            }
            if r["ok"]["done"] == true {
                break;
            }
        }
        assert!(urls[0].ends_with("/ada") && urls[1].ends_with("/bob"), "{urls:?}");
    }
}
