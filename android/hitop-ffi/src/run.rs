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
    let i = inputs(a)?;
    let iterations = a.0.get("iterations").and_then(Value::as_u64).unwrap_or(1).clamp(1, 100) as usize;
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
        let m = masker(&i);
        let steps_per_iteration = i.collection.requests.iter().filter(|r| is_run_step(r)).count();
        let mut builder = ReportBuilder::new(&tool);
        builder.collection(&i.collection.name, i.environment.as_ref().map(|e| e.name.as_str()));
        let mut bailed = false;
        let mut cancelled = false;
        'iterations: for it in 0..iterations {
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
                    builder.iteration(None, &steps);
                    bailed = true;
                    break 'iterations;
                }
            }
            builder.iteration(None, &steps);
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
