// A collection run as `hitop run` reports it (spec/run-report.md, reportVersion 1), built from the
// web app's run results, plus what the Run report page shows: p95, the shared time axis, the
// exact CLI command and a small run history. Pure (no DOM), unit-checked in tests/unit-ux.mjs.

const HISTORY_KEY = "hitop_run_history";
const HISTORY_PER_COLLECTION = 20;

/** The p-th percentile (nearest rank) of a list of numbers, or null when empty. */
export function percentile(values, p = 95) {
  const v = values.filter((x) => Number.isFinite(x)).sort((a, b) => a - b);
  if (!v.length) return null;
  const rank = Math.ceil((p / 100) * v.length);
  return v[Math.min(v.length, Math.max(1, rank)) - 1];
}

/** A shell word, quoted only when it needs to be (POSIX single quotes). */
export function shellWord(s) {
  s = String(s);
  return /^[A-Za-z0-9_./:@%+=,-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
}

/**
 * The `hitop run` line that reproduces this run: the folder when the collection is one, else its
 * name in global storage; the environment by name; `--bail`, since the app's run stops at the
 * first failure; JUnit and JSON reports into `reports`.
 */
export function runCommand({ collection, folderLabel, environment }) {
  const parts = ["hitop", "run", shellWord(folderLabel ? `./${folderLabel.replace(/^\.?\//, "")}` : collection)];
  if (environment) parts.push("--env", shellWord(environment));
  parts.push("--bail", "--reporter", "cli,junit,json", "--out", "reports");
  return parts.join(" ");
}

const isoNow = (ms) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");

/**
 * report.json (reportVersion 1) for one collection's run. `results` are the store's run items:
 * {requestName, method, url, statusCode, success, tests, response, duration, timings, file}.
 */
export function buildReport({ collection, environment, results, startedAt, durationMs, tool = "hitop-web" }) {
  const requests = results.map((r) => {
    const responded = typeof r.statusCode === "number";
    const assertions = (r.tests || []).map((t) => ({ name: t.name, passed: Boolean(t.pass), message: t.pass ? null : t.detail || "failed" }));
    const failedChecks = assertions.filter((a) => !a.passed);
    const failure = r.success
      ? null
      : !responded
        ? r.response || "not sent"
        : failedChecks.length
          ? failedChecks.map((a) => `${a.name}: ${a.message}`).join("; ")
          : `HTTP ${r.statusCode}`;
    const out = {
      name: r.requestName,
      method: r.method,
      url: r.url,
      passed: Boolean(r.success),
      status: responded ? r.statusCode : null,
      responseTimeMs: responded ? (r.timings?.totalMs ?? r.duration ?? null) : null,
      durationMs: r.duration ?? 0,
      error: responded ? null : r.response || "not sent",
      failure,
      assertions,
      file: r.file ?? null,
    };
    if (r.timings) out.timings = r.timings;
    return out;
  });
  const passed = requests.every((r) => r.passed);
  const checks = requests.flatMap((r) => r.assertions);
  return {
    reportVersion: 1,
    tool,
    startedAt: isoNow(startedAt),
    durationMs,
    passed,
    bailed: !passed,
    summary: {
      requests: { total: requests.length, passed: requests.filter((r) => r.passed).length, failed: requests.filter((r) => !r.passed).length },
      assertions: { total: checks.length, passed: checks.filter((a) => a.passed).length, failed: checks.filter((a) => !a.passed).length },
    },
    collections: [{ name: collection, environment: environment ?? null, iterations: [{ iteration: 1, passed, durationMs, requests }] }],
  };
}

const xml = (s) => String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** JUnit XML in the shape `hitop run --reporter junit` writes: one testcase per request. */
export function junitXml(report) {
  const col = report.collections[0];
  const reqs = col.iterations[0].requests;
  const failures = reqs.filter((r) => !r.passed && !r.error).length;
  const errors = reqs.filter((r) => r.error).length;
  const secs = (ms) => (ms / 1000).toFixed(3);
  const cases = reqs.map((r) => {
    const attrs = `name="${xml(`${r.method} ${r.name}`)}" classname="${xml(col.name)}" time="${secs(r.durationMs)}"${r.file ? ` file="${xml(r.file)}"` : ""}`;
    if (r.error) return `    <testcase ${attrs}>\n      <error message="${xml(r.error)}"/>\n    </testcase>`;
    if (!r.passed) return `    <testcase ${attrs}>\n      <failure message="${xml(r.failure)}"/>\n    </testcase>`;
    return `    <testcase ${attrs}/>`;
  });
  return `<?xml version="1.0" encoding="UTF-8"?>\n<testsuites tests="${reqs.length}" failures="${failures}" errors="${errors}" time="${secs(report.durationMs)}">\n  <testsuite name="${xml(col.name)}" tests="${reqs.length}" failures="${failures}" errors="${errors}" time="${secs(report.durationMs)}">\n${cases.join("\n")}\n  </testsuite>\n</testsuites>\n`;
}

/**
 * What the timeline draws: each request's bar on one axis from the run's start. Requests run one
 * after another, so a bar starts where the previous step ended (scripts included).
 */
export function timeline(results) {
  let at = 0;
  const rows = results.map((r) => {
    const ms = r.duration ?? 0;
    const row = { ...r, start: at, ms };
    at += ms;
    return row;
  });
  const axis = Math.max(at, 1);
  return rows.map((r) => ({ ...r, left: (r.start / axis) * 100, width: Math.max((r.ms / axis) * 100, 0.6) }));
}

/** Requests slower than the run's p95 (and at least 200 ms) count as slow. */
export function slowThreshold(results) {
  const p = percentile(results.map((r) => r.duration));
  return Math.max(p ?? 0, 200);
}

/** The run's numbers for the stat tiles, with the p95 change against the previous run. */
export function runStats(results, durationMs, previous) {
  const checks = results.flatMap((r) => r.tests || []);
  const p95 = percentile(results.filter((r) => typeof r.statusCode === "number").map((r) => r.timings?.totalMs ?? r.duration));
  return {
    requests: { passed: results.filter((r) => r.success).length, total: results.length },
    checks: { passed: checks.filter((t) => t.pass).length, total: checks.length },
    p95,
    p95Delta: p95 != null && previous?.p95 != null ? p95 - previous.p95 : null,
    wallMs: durationMs,
  };
}

function readHistory(storage) {
  try {
    const v = JSON.parse(storage.getItem(HISTORY_KEY) || "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

/** The last run of a collection before this one, from `hitop_run_history`. */
export function previousRun(storage, collectionId) {
  const list = readHistory(storage)[String(collectionId)] || [];
  return list[list.length - 1] ?? null;
}

/** Remembers a run's summary (never bodies or URLs): `hitop_run_history`, 20 per collection. */
export function rememberRun(storage, collectionId, entry) {
  try {
    const all = readHistory(storage);
    const list = [...(all[String(collectionId)] || []), entry].slice(-HISTORY_PER_COLLECTION);
    all[String(collectionId)] = list;
    storage.setItem(HISTORY_KEY, JSON.stringify(all));
  } catch {
    // Storage full or unavailable: the report still shows, without a comparison next time.
  }
}

const STATUS_CAUSES = [
  [401, "The server wants credentials it didn't get or didn't accept: check the request's auth and the token's variable."],
  [403, "The credentials were accepted but don't allow this: check the account's role or the token's scopes."],
  [404, "Nothing answers at this path: check the URL as sent and the variables in it."],
  [405, "The path exists but not with this method."],
  [408, "The server gave up waiting for the request."],
  [409, "The server refused it as a conflict with its current state (often data an earlier run left behind)."],
  [415, "The server doesn't take this body type: check the Content-Type header."],
  [422, "The server understood the body but refused its content: the response body says which field."],
  [429, "Too many requests: the server is rate limiting this run."],
];

/**
 * U39: why a run's failed request failed, from what the run knows (nothing guessed beyond the
 * status code and the error's text): `{ kind, status, message, cause, checks, excerpt, url }`.
 * kind: "not-sent" (stopped before sending), "no-answer" (sent, no response), "checks" (failed
 * tests), "status" (no tests, a status outside 2xx). `cause` is null when nothing more is known.
 */
export function explainFailure(r) {
  const responded = typeof r.statusCode === "number";
  const url = r.url ?? "";
  if (!responded) {
    const message = String(r.response || "not sent");
    const missingVar = /Variable '([^']+)' not found/.exec(message);
    let kind = "no-answer";
    let cause = null;
    if (r.statusCode === "Not sent") kind = "not-sent";
    else if (missingVar) {
      kind = "not-sent";
      cause = `\${${missingVar[1]}} has no value: no active environment, collection variable or global defines it, so nothing was sent.`;
    } else if (r.statusCode === "Script error") {
      kind = "not-sent";
      cause = "The script before the request stopped with an error, so nothing was sent.";
    } else if (/No response within|timed? ?out|TimeoutError/i.test(message)) cause = "No answer within the time limit (Settings, under Requests).";
    else if (/Failed to fetch|NetworkError|ECONNREFUSED|connection refused|error sending request|dns|resolve|unreachable|ENOTFOUND/i.test(message)) {
      let host = "";
      try {
        host = new URL(url).host;
      } catch {
        // a URL with unfilled variables
      }
      cause = `No answer${host ? ` from ${host}` : ""}: is the server running, and is the address right?`;
    }
    return { kind, status: null, message, cause, checks: [], excerpt: "", url };
  }
  const checks = (r.tests || []).filter((t) => !t.pass);
  const status = r.statusCode;
  const excerpt = String(r.excerpt ?? "").trim();
  if (checks.length) return { kind: "checks", status, message: "", cause: null, checks, excerpt, url };
  const known = STATUS_CAUSES.find(([code]) => code === status)?.[1];
  const cause = known ?? (status >= 500 ? "The server failed while answering: its response body or logs say why." : status >= 300 && status < 400 ? "A redirect that wasn't followed: the Location header says where to." : null);
  return { kind: "status", status, message: "", cause, checks: [], excerpt, url };
}
