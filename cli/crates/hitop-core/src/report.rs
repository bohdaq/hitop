#![cfg(feature = "native")]
//! Run reports: the JSON report (`spec/run-report.md`, `reportVersion` 1)
//! and JUnit XML, built from the runner's steps. Steps come from
//! [`crate::runner::run_collection_with`] or `_opts` already masked, so a
//! report never holds a secret value.
//!
//! No dependencies beyond serde: the XML and the timestamps are written
//! here.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::runner::RunStepResult;

/// The JSON report's version. Fields may be added within a version;
/// removing or changing one bumps it.
pub const REPORT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub report_version: u32,
    pub tool: Tool,
    /// RFC 3339, UTC, whole seconds.
    pub started_at: String,
    pub duration_ms: u64,
    /// Every request passed (sent, status below 400, every assertion true).
    pub passed: bool,
    /// The run stopped at the first failure (`--bail`) before the end.
    pub bailed: bool,
    pub summary: Summary,
    pub collections: Vec<CollectionReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub collections: usize,
    pub iterations: usize,
    pub requests: Counts,
    pub assertions: Counts,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Counts {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

impl Counts {
    fn add(&mut self, passed: bool) {
        self.total += 1;
        if passed {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionReport {
    pub name: String,
    /// The environment's name, or null without one.
    pub environment: Option<String>,
    pub passed: bool,
    pub iterations: Vec<IterationReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IterationReport {
    /// 1-based.
    pub iteration: usize,
    pub started_at: String,
    pub duration_ms: u64,
    /// The `--data` row this iteration ran with, or null.
    pub data: Option<BTreeMap<String, String>>,
    pub passed: bool,
    pub requests: Vec<RequestReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestReport {
    pub name: String,
    pub method: String,
    /// As sent, secrets masked.
    pub url: String,
    pub passed: bool,
    /// HTTP status, or null when no response came (see `error`).
    pub status: Option<u16>,
    /// Time to the response headers, or null without a response.
    pub response_time_ms: Option<u64>,
    /// The whole step: scripts, request and response.
    pub duration_ms: u64,
    /// Why the request wasn't sent or answered (connection, timeout,
    /// missing variable, script error), or null.
    pub error: Option<String>,
    /// One line on why the request failed, or null when it passed.
    pub failure: Option<String>,
    pub assertions: Vec<AssertionReport>,
    /// The request's file when the collection was run from a folder: the
    /// folder as given on the command line, then `requests/<file>.yaml`
    /// (`api/requests/orders/create.yaml`); null otherwise. Added in hitop
    /// 0.3.0, within reportVersion 1. Older reports have no key.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssertionReport {
    pub name: String,
    pub passed: bool,
    /// What was found instead, or null when it passed.
    pub message: Option<String>,
}

impl RequestReport {
    pub fn from_step(step: &RunStepResult) -> RequestReport {
        let assertions: Vec<AssertionReport> = step
            .tests
            .iter()
            .map(|t| AssertionReport {
                name: t.name.clone(),
                passed: t.pass,
                message: (!t.pass).then(|| if t.detail.is_empty() { "failed".to_string() } else { t.detail.clone() }),
            })
            .collect();
        let (status, response_time_ms, error) = match &step.result {
            Ok(r) => (Some(r.status), Some(r.duration_ms), None),
            Err(e) => (None, None, Some(format!("{e:#}"))),
        };
        let passed = step.passed();
        let failed: Vec<String> = assertions
            .iter()
            .filter(|a| !a.passed)
            .map(|a| format!("{}: {}", a.name, a.message.as_deref().unwrap_or("failed")))
            .collect();
        let failure = if passed {
            None
        } else if let Some(e) = &error {
            Some(e.clone())
        } else if !failed.is_empty() {
            Some(failed.join("; "))
        } else {
            Some(format!("HTTP {}", status.unwrap_or_default()))
        };
        RequestReport {
            name: step.request_name.clone(),
            method: step.method.clone(),
            url: step.url.clone(),
            passed,
            status,
            response_time_ms,
            duration_ms: step.duration_ms,
            error,
            failure,
            assertions,
            file: None,
        }
    }
}

/// Collects iterations while a run goes, then [`Report`].
pub struct ReportBuilder {
    started: SystemTime,
    clock: Instant,
    iteration_start: Option<(SystemTime, Instant)>,
    tool_version: String,
    collections: Vec<CollectionReport>,
    /// The last collection's request files, by request index.
    files: Vec<String>,
}

impl ReportBuilder {
    /// `tool_version` is the hitop version that ran.
    pub fn new(tool_version: &str) -> ReportBuilder {
        ReportBuilder {
            started: SystemTime::now(),
            clock: Instant::now(),
            iteration_start: None,
            tool_version: tool_version.into(),
            collections: vec![],
            files: vec![],
        }
    }

    /// Starts a collection; its iterations follow.
    pub fn collection(&mut self, name: &str, environment: Option<&str>) {
        self.collections.push(CollectionReport {
            name: name.into(),
            environment: environment.map(str::to_string),
            passed: true,
            iterations: vec![],
        });
        self.files.clear();
    }

    /// The last collection's request files, in request order (a folder
    /// run): each request's report and `<testcase>` get its `file`.
    pub fn request_files(&mut self, files: Vec<String>) {
        self.files = files;
    }

    /// Marks the start of an iteration, for its timestamp and duration.
    pub fn start_iteration(&mut self) {
        self.iteration_start = Some((SystemTime::now(), Instant::now()));
    }

    /// Adds an iteration's steps to the last collection.
    pub fn iteration(&mut self, data: Option<BTreeMap<String, String>>, steps: &[RunStepResult]) {
        let (at, clock) = self.iteration_start.take().unwrap_or((SystemTime::now(), Instant::now()));
        if self.collections.is_empty() {
            self.collection("", None);
        }
        let col = self.collections.last_mut().expect("a collection");
        // Steps are the collection's requests in order (a prefix of them
        // when the run stopped early), so the index finds the file.
        let requests: Vec<RequestReport> = steps
            .iter()
            .enumerate()
            .map(|(i, s)| RequestReport { file: self.files.get(i).cloned(), ..RequestReport::from_step(s) })
            .collect();
        let passed = requests.iter().all(|r| r.passed);
        col.passed &= passed;
        col.iterations.push(IterationReport {
            iteration: col.iterations.len() + 1,
            started_at: rfc3339(at),
            duration_ms: clock.elapsed().as_millis() as u64,
            data,
            passed,
            requests,
        });
    }

    pub fn finish(self, bailed: bool) -> Report {
        let mut summary = Summary { collections: self.collections.len(), ..Default::default() };
        for c in &self.collections {
            for i in &c.iterations {
                summary.iterations += 1;
                for r in &i.requests {
                    summary.requests.add(r.passed);
                    for a in &r.assertions {
                        summary.assertions.add(a.passed);
                    }
                }
            }
        }
        Report {
            report_version: REPORT_VERSION,
            tool: Tool { name: "hitop".into(), version: self.tool_version },
            started_at: rfc3339(self.started),
            duration_ms: self.clock.elapsed().as_millis() as u64,
            passed: summary.requests.failed == 0,
            bailed,
            summary,
            collections: self.collections,
        }
    }
}

impl Report {
    /// Pretty-printed JSON, ending with a newline.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("a report serializes");
        s.push('\n');
        s
    }

    /// JUnit XML: `<testsuites>` with one `<testsuite>` per collection and
    /// iteration and one `<testcase>` per request. A failed request has one
    /// `<failure>` (assertions, HTTP status) or one `<error>` (no response);
    /// its text lists every assertion.
    pub fn to_junit(&self) -> String {
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        let (mut tests, mut failures, mut errors) = (0, 0, 0);
        for c in &self.collections {
            for i in &c.iterations {
                for r in &i.requests {
                    tests += 1;
                    match (r.passed, &r.error) {
                        (true, _) => {}
                        (false, Some(_)) => errors += 1,
                        (false, None) => failures += 1,
                    }
                }
            }
        }
        let _ = writeln!(
            x,
            "<testsuites name=\"hitop\" tests=\"{tests}\" failures=\"{failures}\" errors=\"{errors}\" time=\"{}\" timestamp=\"{}\">",
            seconds(self.duration_ms),
            junit_time(&self.started_at),
        );
        for c in &self.collections {
            let many = c.iterations.len() > 1;
            for i in &c.iterations {
                let name = if many { format!("{} (iteration {})", c.name, i.iteration) } else { c.name.clone() };
                let failures = i.requests.iter().filter(|r| !r.passed && r.error.is_none()).count();
                let errors = i.requests.iter().filter(|r| !r.passed && r.error.is_some()).count();
                let _ = writeln!(
                    x,
                    "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"0\" time=\"{}\" timestamp=\"{}\">",
                    attr(&name),
                    i.requests.len(),
                    seconds(i.duration_ms),
                    junit_time(&i.started_at),
                );
                let mut props: Vec<(String, String)> = vec![("iteration".into(), i.iteration.to_string())];
                if let Some(env) = &c.environment {
                    props.push(("environment".into(), env.clone()));
                }
                for (k, v) in i.data.iter().flatten() {
                    props.push((format!("data.{k}"), v.clone()));
                }
                x.push_str("    <properties>\n");
                for (k, v) in props {
                    let _ = writeln!(x, "      <property name=\"{}\" value=\"{}\"/>", attr(&k), attr(&v));
                }
                x.push_str("    </properties>\n");
                for r in &i.requests {
                    let file = r.file.as_deref().map(|f| format!(" file=\"{}\"", attr(f))).unwrap_or_default();
                    let _ = write!(
                        x,
                        "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\"{file}>",
                        attr(&r.name),
                        attr(&c.name),
                        seconds(r.duration_ms)
                    );
                    let detail = text(&case_text(r));
                    if r.passed {
                        let _ = writeln!(x, "\n      <system-out>{detail}</system-out>\n    </testcase>");
                        continue;
                    }
                    let message = attr(r.failure.as_deref().unwrap_or("failed"));
                    let (tag, kind) = match &r.error {
                        Some(_) => ("error", "RequestError"),
                        None if r.assertions.iter().any(|a| !a.passed) => ("failure", "AssertionError"),
                        None => ("failure", "HttpStatus"),
                    };
                    let _ = writeln!(x, "\n      <{tag} message=\"{message}\" type=\"{kind}\">{detail}</{tag}>\n    </testcase>");
                }
                x.push_str("  </testsuite>\n");
            }
        }
        x.push_str("</testsuites>\n");
        x
    }
}

const HTML_STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#fff;--fg:#1b1d22;--muted:#5d6370;--line:#d9dce3;--card:#f5f6f8;--pass:#17803d;--pass-bg:#e3f5e9;--fail:#c4261d;--fail-bg:#fde7e5;--bar:#8aa4d6;--code:#eef0f4}
@media (prefers-color-scheme:dark){:root{--bg:#121418;--fg:#e6e8ec;--muted:#9aa1ad;--line:#2c3038;--card:#1b1e24;--pass:#5fd38a;--pass-bg:#16301f;--fail:#ff7b72;--fail-bg:#3a1a19;--bar:#4f6aa3;--code:#22262e}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
.page{max-width:1100px;margin:0 auto;padding:24px 16px 48px}
h1{font-size:22px;margin:0 0 4px}
h2{font-size:18px;margin:32px 0 8px}
h3{font-size:15px;margin:20px 0 8px;color:var(--muted)}
.meta{color:var(--muted);margin:0 0 16px}
.badge{display:inline-block;padding:1px 8px;border-radius:4px;font-size:12px;font-weight:600;vertical-align:middle}
.badge.pass{background:var(--pass-bg);color:var(--pass)}
.badge.fail{background:var(--fail-bg);color:var(--fail)}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin:16px 0}
.card{background:var(--card);border:1px solid var(--line);border-radius:6px;padding:12px}
.card b{display:block;font-size:20px}
.card span{color:var(--muted);font-size:12px}
.note{background:var(--fail-bg);color:var(--fail);border-radius:6px;padding:8px 12px}
.filter{display:inline-flex;gap:6px;align-items:center;margin:8px 0;cursor:pointer}
body:has(#failures-only:checked) .req.pass{display:none}
table.data{border-collapse:collapse;margin:0 0 8px}
table.data td,table.data th{border:1px solid var(--line);padding:2px 8px;text-align:left;font-weight:normal}
table.data th{color:var(--muted)}
.req{border:1px solid var(--line);border-radius:6px;margin:6px 0;background:var(--bg)}
.req.fail{border-color:var(--fail)}
.req summary{display:grid;grid-template-columns:20px 64px minmax(0,1fr) 56px 140px;gap:8px;align-items:center;padding:6px 10px;cursor:pointer;list-style:none}
.req summary::-webkit-details-marker{display:none}
.mark.pass{color:var(--pass)}
.mark.fail{color:var(--fail)}
.method{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:12px;color:var(--muted)}
.name{overflow-wrap:anywhere}
.status{text-align:right;font-variant-numeric:tabular-nums}
.time{font-size:12px;color:var(--muted);font-variant-numeric:tabular-nums}
.time i{display:block;height:4px;border-radius:2px;background:var(--bar);margin-top:2px}
.body{padding:4px 12px 10px 38px;border-top:1px solid var(--line)}
.body dl{display:grid;grid-template-columns:max-content minmax(0,1fr);gap:2px 12px;margin:6px 0}
.body dt{color:var(--muted)}
.body dd{margin:0;overflow-wrap:anywhere}
code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:12px;background:var(--code);padding:1px 4px;border-radius:3px}
.failure{color:var(--fail)}
ul.asserts{list-style:none;padding:0;margin:6px 0}
ul.asserts li{padding:1px 0}
header.page{padding-bottom:0}
main.page{padding-top:0}
footer{margin-top:40px;color:var(--muted);font-size:12px}
@media (max-width:640px){.req summary{grid-template-columns:20px minmax(0,1fr) 48px}.req summary .method,.req summary .time{display:none}}
"#;

impl Report {
    /// One self-contained HTML page: inline CSS, no script, and a Content
    /// Security Policy that forbids loading anything, so it works offline
    /// and opens no connection. A pure function of the report (no clock),
    /// so the same report gives the same bytes. Every string from the
    /// report is escaped; secret values are already `••••`.
    pub fn to_html(&self) -> String {
        let mut h = String::with_capacity(16 * 1024);
        let s = &self.summary;
        let verdict = if self.passed { ("pass", "Passed") } else { ("fail", "Failed") };
        h.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
        h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        h.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">\n");
        h.push_str("<meta name=\"referrer\" content=\"no-referrer\">\n");
        let title = match self.collections.as_slice() {
            [one] => format!("hitop run: {}", one.name),
            _ => "hitop run".to_string(),
        };
        let _ = writeln!(h, "<title>{} ({})</title>", html(&title), verdict.1.to_lowercase());
        let _ = writeln!(h, "<style>{HTML_STYLE}</style>\n</head>\n<body>");
        let _ = writeln!(h, "<header class=\"page\">\n<h1>{} <span class=\"badge {}\">{}</span></h1>", html(&title), verdict.0, verdict.1);
        let _ = writeln!(
            h,
            "<p class=\"meta\">Started <time datetime=\"{0}\">{0}</time> · {1} · {2} {3}</p>",
            html(&self.started_at),
            millis(self.duration_ms),
            html(&self.tool.name),
            html(&self.tool.version),
        );
        if self.bailed {
            h.push_str("<p class=\"note\">The run stopped at the first failure (--bail): later requests and iterations did not run.</p>\n");
        }
        h.push_str("<div class=\"cards\">\n");
        let card = |h: &mut String, value: String, label: &str| {
            let _ = writeln!(h, "<div class=\"card\"><b>{value}</b><span>{label}</span></div>");
        };
        card(&mut h, format!("{}/{}", s.requests.passed, s.requests.total), "requests passed");
        card(&mut h, format!("{}/{}", s.assertions.passed, s.assertions.total), "assertions passed");
        card(&mut h, s.requests.failed.to_string(), "requests failed");
        card(&mut h, s.collections.to_string(), if s.collections == 1 { "collection" } else { "collections" });
        card(&mut h, s.iterations.to_string(), if s.iterations == 1 { "iteration" } else { "iterations" });
        card(&mut h, millis(self.duration_ms), "duration");
        h.push_str("</div>\n");
        // Hides passing requests through CSS `:has()`, no script.
        h.push_str("<label class=\"filter\"><input type=\"checkbox\" id=\"failures-only\"> Show failed requests only</label>\n");
        h.push_str("</header>\n");
        h.push_str("<main class=\"page\">\n");
        for c in &self.collections {
            let v = if c.passed { ("pass", "Passed") } else { ("fail", "Failed") };
            let _ = writeln!(h, "<section class=\"collection\">\n<h2>{} <span class=\"badge {}\">{}</span></h2>", html(&c.name), v.0, v.1);
            match &c.environment {
                Some(e) => {
                    let _ = writeln!(h, "<p class=\"meta\">Environment <code>{}</code></p>", html(e));
                }
                None => h.push_str("<p class=\"meta\">No environment</p>\n"),
            }
            let many = c.iterations.len() > 1;
            for i in &c.iterations {
                if many || i.data.is_some() {
                    let v = if i.passed { ("pass", "passed") } else { ("fail", "failed") };
                    let _ = writeln!(
                        h,
                        "<h3>Iteration {n} <span class=\"badge {c}\">{t}</span> · <time datetime=\"{at}\">{at}</time> · {d}</h3>",
                        n = i.iteration,
                        c = v.0,
                        t = v.1,
                        at = html(&i.started_at),
                        d = millis(i.duration_ms),
                    );
                }
                if let Some(data) = &i.data {
                    h.push_str("<table class=\"data\">\n<tr>");
                    for k in data.keys() {
                        let _ = write!(h, "<th>{}</th>", html(k));
                    }
                    h.push_str("</tr>\n<tr>");
                    for v in data.values() {
                        let _ = write!(h, "<td>{}</td>", html(v));
                    }
                    h.push_str("</tr>\n</table>\n");
                }
                let longest = i.requests.iter().map(|r| r.duration_ms).max().unwrap_or(0).max(1);
                for r in &i.requests {
                    html_request(&mut h, r, longest);
                }
            }
            h.push_str("</section>\n");
        }
        h.push_str("<footer>Report format: hitop run report, version 1 (spec/run-report.md). Secret values show as ••••.</footer>\n");
        h.push_str("</main>\n</body>\n</html>\n");
        h
    }
}

fn html_request(h: &mut String, r: &RequestReport, longest: u64) {
    let (class, mark) = if r.passed { ("pass", "✓") } else { ("fail", "✗") };
    let open = if r.passed { "" } else { " open" };
    let status = match r.status {
        Some(s) => s.to_string(),
        None => "—".to_string(),
    };
    // A whole number from 1 to 100: the only value in a style attribute.
    let width = (r.duration_ms.saturating_mul(100) / longest).clamp(1, 100);
    let _ = writeln!(
        h,
        "<details class=\"req {class}\"{open}>\n<summary><span class=\"mark {class}\">{mark}</span><span class=\"method\">{}</span><span class=\"name\">{}</span><span class=\"status\">{status}</span><span class=\"time\">{}<i style=\"width:{width}%\"></i></span></summary>",
        html(&r.method),
        html(&r.name),
        millis(r.duration_ms),
    );
    h.push_str("<div class=\"body\">\n<dl>\n");
    let _ = writeln!(h, "<dt>URL</dt><dd><code>{}</code></dd>", html(&r.url));
    if let Some(f) = &r.file {
        let _ = writeln!(h, "<dt>File</dt><dd><code>{}</code></dd>", html(f));
    }
    if let Some(ms) = r.response_time_ms {
        let _ = writeln!(h, "<dt>Response time</dt><dd>{}</dd>", millis(ms));
    }
    let _ = writeln!(h, "<dt>Duration</dt><dd>{}</dd>", millis(r.duration_ms));
    if let Some(e) = &r.error {
        let _ = writeln!(h, "<dt>Error</dt><dd class=\"failure\">{}</dd>", html(e));
    }
    if let Some(f) = &r.failure {
        let _ = writeln!(h, "<dt>Failure</dt><dd class=\"failure\">{}</dd>", html(f));
    }
    h.push_str("</dl>\n");
    if !r.assertions.is_empty() {
        h.push_str("<ul class=\"asserts\">\n");
        for a in &r.assertions {
            match &a.message {
                None if a.passed => {
                    let _ = writeln!(h, "<li><span class=\"mark pass\">✓</span> {}</li>", html(&a.name));
                }
                m => {
                    let _ = writeln!(
                        h,
                        "<li><span class=\"mark fail\">✗</span> {}: <span class=\"failure\">{}</span></li>",
                        html(&a.name),
                        html(m.as_deref().unwrap_or("failed")),
                    );
                }
            }
        }
        h.push_str("</ul>\n");
    }
    h.push_str("</div>\n</details>\n");
}

/// `182 ms`, `1.234 s`.
fn millis(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{} s", seconds(ms))
    }
}

/// HTML text and attribute values (always double-quoted): the five
/// markup characters become references; NUL and other C0 controls but
/// tab, newline and carriage return become U+FFFD, as HTML forbids them.
fn html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => out.push('\u{FFFD}'),
            _ => out.push(c),
        }
    }
    out
}

/// The request line, then one line per assertion.
fn case_text(r: &RequestReport) -> String {
    let mut s = match (r.status, &r.error) {
        (Some(status), _) => format!("{} {} -> {status} ({} ms)", r.method, r.url, r.response_time_ms.unwrap_or_default()),
        (None, Some(e)) => format!("{} {} -> {e}", r.method, r.url),
        (None, None) => format!("{} {}", r.method, r.url),
    };
    for a in &r.assertions {
        match &a.message {
            None => {
                let _ = write!(s, "\n✓ {}", a.name);
            }
            Some(m) => {
                let _ = write!(s, "\n✗ {}: {m}", a.name);
            }
        }
    }
    s
}

fn seconds(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// JUnit's `timestamp` is an ISO 8601 date-time without a zone (the time
/// is UTC).
fn junit_time(rfc3339: &str) -> &str {
    rfc3339.strip_suffix('Z').unwrap_or(rfc3339)
}

/// Characters XML 1.0 can't hold are dropped.
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

fn text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().filter(|c| xml_char(*c)) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().filter(|c| xml_char(*c)) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
    out
}

/// `2026-11-01T12:00:00Z`.
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RequestResult, TestResult};
    use std::time::Duration;

    fn step(name: &str, status: Option<u16>, tests: &[(&str, bool, &str)]) -> RunStepResult {
        RunStepResult {
            request_name: name.into(),
            method: "GET".into(),
            url: format!("http://h/{name}"),
            result: match status {
                Some(s) => Ok(RequestResult { status: s, headers: Default::default(), body: String::new(), duration_ms: 7, timings: None }),
                None => Err(anyhow::anyhow!("connection refused")),
            },
            tests: tests.iter().map(|(n, p, d)| TestResult { name: n.to_string(), pass: *p, detail: d.to_string() }).collect(),
            duration_ms: 1234,
        }
    }

    fn sample() -> Report {
        let mut b = ReportBuilder::new("9.9.9");
        b.collection("Shop & <API>", Some("dev"));
        b.start_iteration();
        b.iteration(
            None,
            &[
                step("ok", Some(200), &[("status is 200", true, "")]),
                step("bad", Some(200), &[("status is 201", false, "got 200"), ("ok", true, "")]),
                step("down", None, &[]),
                step("500", Some(500), &[]),
            ],
        );
        b.finish(false)
    }

    #[test]
    fn rfc3339_formats_utc() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(UNIX_EPOCH + Duration::from_secs(1_772_323_200 + 3_723)), "2026-03-01T01:02:03Z");
        assert_eq!(rfc3339(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn summary_counts_requests_and_assertions() {
        let r = sample();
        assert!(!r.passed);
        assert_eq!(r.summary.requests, Counts { total: 4, passed: 1, failed: 3 });
        assert_eq!(r.summary.assertions, Counts { total: 3, passed: 2, failed: 1 });
        let reqs = &r.collections[0].iterations[0].requests;
        assert_eq!(reqs[1].failure.as_deref(), Some("status is 201: got 200"));
        assert_eq!(reqs[2].failure.as_deref(), Some("connection refused"));
        assert_eq!(reqs[3].failure.as_deref(), Some("HTTP 500"));
        assert_eq!(reqs[0].response_time_ms, Some(7));
        assert_eq!(reqs[2].status, None);
    }

    #[test]
    fn json_is_versioned_camel_case_and_reads_back() {
        let r = sample();
        let json = r.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["reportVersion"], 1);
        assert_eq!(v["tool"]["version"], "9.9.9");
        assert_eq!(v["collections"][0]["iterations"][0]["requests"][0]["responseTimeMs"], 7);
        assert!(v["collections"][0]["iterations"][0]["data"].is_null());
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn junit_escapes_and_counts() {
        let x = sample().to_junit();
        assert!(x.contains("<testsuites name=\"hitop\" tests=\"4\" failures=\"2\" errors=\"1\""), "{x}");
        assert!(x.contains("<testsuite name=\"Shop &amp; &lt;API&gt;\" tests=\"4\" failures=\"2\" errors=\"1\" skipped=\"0\" time=\"0."));
        assert!(x.contains("<testcase name=\"bad\" classname=\"Shop &amp; &lt;API&gt;\" time=\"1.234\">"));
        assert!(x.contains("<failure message=\"status is 201: got 200\" type=\"AssertionError\">GET http://h/bad -&gt; 200 (7 ms)\n✗ status is 201: got 200\n✓ ok</failure>"), "{x}");
        assert!(x.contains("<error message=\"connection refused\" type=\"RequestError\">"));
        assert!(x.contains("<failure message=\"HTTP 500\" type=\"HttpStatus\">"));
        assert!(x.contains("<property name=\"environment\" value=\"dev\"/>"));
    }

    #[test]
    fn request_files_reach_the_report_and_junit() {
        let mut b = ReportBuilder::new("1");
        b.collection("A", None);
        b.request_files(vec!["api/requests/log-in.yaml".into(), "api/requests/a&b.yaml".into(), "api/requests/unrun.yaml".into()]);
        b.start_iteration();
        b.iteration(None, &[step("log in", Some(200), &[]), step("a&b", Some(500), &[])]);
        // A collection without files (global storage) has none.
        b.collection("B", None);
        b.start_iteration();
        b.iteration(None, &[step("x", Some(200), &[])]);
        let r = b.finish(false);
        let reqs = &r.collections[0].iterations[0].requests;
        assert_eq!(reqs[0].file.as_deref(), Some("api/requests/log-in.yaml"));
        assert_eq!(r.collections[1].iterations[0].requests[0].file, None);
        let x = r.to_junit();
        assert!(x.contains("<testcase name=\"log in\" classname=\"A\" time=\"1.234\" file=\"api/requests/log-in.yaml\">"), "{x}");
        assert!(x.contains("file=\"api/requests/a&amp;b.yaml\""), "{x}");
        assert!(x.contains("<testcase name=\"x\" classname=\"B\" time=\"1.234\">"), "{x}");
        // JSON: null without a file; a report without the key still reads.
        let json: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        assert_eq!(json["collections"][0]["iterations"][0]["requests"][0]["file"], "api/requests/log-in.yaml");
        assert!(json["collections"][1]["iterations"][0]["requests"][0]["file"].is_null());
        let mut old = json.clone();
        old["collections"][0]["iterations"][0]["requests"][0].as_object_mut().unwrap().remove("file");
        let back: Report = serde_json::from_value(old).unwrap();
        assert_eq!(back.collections[0].iterations[0].requests[0].file, None);
    }

    #[test]
    fn xml_drops_characters_it_cannot_hold() {
        assert_eq!(attr("a\u{1}b\"\n"), "ab&quot;&#10;");
        assert_eq!(text("x\u{0}<y>"), "x&lt;y&gt;");
    }

    #[test]
    fn several_iterations_are_suites_with_their_data() {
        let mut b = ReportBuilder::new("1");
        b.collection("C", None);
        for user in ["ada", "bob"] {
            b.start_iteration();
            b.iteration(Some([("user".to_string(), user.to_string())].into()), &[step("r", Some(200), &[])]);
        }
        let r = b.finish(false);
        assert_eq!(r.summary.iterations, 2);
        let x = r.to_junit();
        assert!(x.contains("<testsuite name=\"C (iteration 2)\""));
        assert!(x.contains("<property name=\"data.user\" value=\"bob\"/>"));
    }
}
