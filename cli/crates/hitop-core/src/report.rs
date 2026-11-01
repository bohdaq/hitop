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
        let requests: Vec<RequestReport> = steps.iter().map(RequestReport::from_step).collect();
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
                    let _ = write!(
                        x,
                        "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\">",
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
                Some(s) => Ok(RequestResult { status: s, headers: Default::default(), body: String::new(), duration_ms: 7 }),
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
