//! `hitop run` for CI: reporters (cli, junit, json, html), exit codes and the run
//! flags (--env-file, --var, --bail, --timeout, --iterations, --data).
//! HTTP goes to a local mock server only; the keychain is off.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

const SECRET: &str = "s3cr3t-Run_77";

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .env_remove("HITOP_SECRET_CI_TOKEN")
        .current_dir(cwd);
    c
}

/// `api/` with three requests: `ok` (asserts 200), `check` (asserts the
/// body holds "yes"), `who` (`/who/${user}`, no assertion); environment
/// `ci` with `base` and a secret `token` sent as a bearer token.
fn folder(tmp: &TempDir, base: &str) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(
        dir.join("hitop.yaml"),
        "format: 1\nname: Run API\nworkspace: ws-run\nvariables:\n  user: nobody\nauth:\n  type: bearer\n  token: ${token}\norder:\n- ok\n- check\n- who\n",
    )
    .unwrap();
    fs::write(dir.join("requests/ok.yaml"), "name: Ok\nurl: ${base}/ok?key=${token}\nscripts:\n  post: assert.status(200);\n").unwrap();
    fs::write(
        dir.join("requests/check.yaml"),
        "name: Check & <compare>\nurl: ${base}/check\nscripts:\n  post: |\n    assert.status(200);\n    assert.bodyContains('yes');\n",
    )
    .unwrap();
    fs::write(dir.join("requests/who.yaml"), "name: Who\nurl: ${base}/who/${user}\n").unwrap();
    fs::write(dir.join("environments/ci.yaml"), format!("variables:\n  base: {base}\n  token:\n    secret: true\n")).unwrap();
    dir
}

fn all_ok(server: &MockServer) {
    server.mock(|when, then| {
        when.any_request();
        then.status(200).body("yes");
    });
}

fn read_json(path: PathBuf) -> Value {
    serde_json::from_str(&fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

#[test]
fn all_passing_exits_0_and_prints_each_assertion() {
    let server = MockServer::start();
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123"])
        .assert()
        .code(0)
        .stdout(contains("✓ status is 200").and(contains("body contains \"yes\"")).and(contains("3/3 requests succeeded, 3/3 assertions passed")));
    // The cli reporter alone writes no files.
    assert!(!tmp.path().join("hitop-reports").exists());
}

#[test]
fn junit_report_has_the_expected_shape() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/ok");
        then.status(200);
    });
    server.mock(|when, then| {
        when.path("/check");
        then.status(500).body("no");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    // `who` goes to a closed port: an error, not a failure.
    fs::write(dir.join("requests/who.yaml"), "name: Who\nurl: http://127.0.0.1:9/who\n").unwrap();
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--reporter", "junit", "--out", "reports"])
        .assert()
        .code(1)
        .stderr(contains("reports/junit.xml"));
    let xml = fs::read_to_string(tmp.path().join("reports/junit.xml")).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{e}\n{xml}"));

    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "testsuites");
    assert_eq!(root.attribute("tests"), Some("3"));
    assert_eq!(root.attribute("failures"), Some("1"));
    assert_eq!(root.attribute("errors"), Some("1"));
    assert!(root.attribute("time").unwrap().parse::<f64>().is_ok());

    let suites: Vec<_> = root.children().filter(|n| n.is_element()).collect();
    assert_eq!(suites.len(), 1);
    let suite = suites[0];
    assert_eq!(suite.tag_name().name(), "testsuite");
    for (k, v) in [("name", "Run API"), ("tests", "3"), ("failures", "1"), ("errors", "1"), ("skipped", "0")] {
        assert_eq!(suite.attribute(k), Some(v), "testsuite {k}");
    }
    assert!(suite.attribute("time").unwrap().parse::<f64>().is_ok());
    let ts = suite.attribute("timestamp").unwrap();
    assert_eq!(ts.len(), 19, "{ts}");
    assert_eq!(&ts[10..11], "T");

    let cases: Vec<_> = suite.children().filter(|n| n.has_tag_name("testcase")).collect();
    let names: Vec<_> = cases.iter().map(|c| c.attribute("name").unwrap()).collect();
    assert_eq!(names, ["Ok", "Check & <compare>", "Who"]);
    for c in &cases {
        assert_eq!(c.attribute("classname"), Some("Run API"));
        assert!(c.attribute("time").unwrap().parse::<f64>().is_ok());
    }
    // Passing: no failure, the assertions in system-out.
    assert!(cases[0].children().all(|n| !n.has_tag_name("failure") && !n.has_tag_name("error")));
    let out = cases[0].children().find(|n| n.has_tag_name("system-out")).unwrap().text().unwrap();
    assert!(out.contains("✓ status is 200"), "{out}");

    // Failed assertions: one failure element, message and every assertion.
    let failures: Vec<_> = cases[1].children().filter(|n| n.has_tag_name("failure")).collect();
    assert_eq!(failures.len(), 1);
    let f = failures[0];
    assert_eq!(f.attribute("type"), Some("AssertionError"));
    assert_eq!(f.attribute("message"), Some("status is 200: got 500; body contains \"yes\": not found in the response body"));
    let text = f.text().unwrap();
    assert!(text.contains("-> 500") && text.contains("✗ status is 200: got 500") && text.contains("✗ body contains \"yes\""), "{text}");

    // No response: one error element with the reason.
    let error = cases[2].children().find(|n| n.has_tag_name("error")).unwrap();
    assert_eq!(error.attribute("type"), Some("RequestError"));
    assert!(!error.attribute("message").unwrap().is_empty());
}

#[test]
fn json_report_is_versioned_and_complete() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/check");
        then.status(200).body("no");
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--reporter", "cli,json,junit"])
        .assert()
        .code(1)
        .stdout(contains("2/3 requests succeeded"));
    let v = read_json(tmp.path().join("hitop-reports/report.json"));
    assert!(tmp.path().join("hitop-reports/junit.xml").is_file());
    assert_eq!(v["reportVersion"], 1);
    assert_eq!(v["tool"]["name"], "hitop");
    assert_eq!(v["tool"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(v["startedAt"].as_str().unwrap().ends_with('Z'));
    assert_eq!(v["passed"], false);
    assert_eq!(v["bailed"], false);
    assert_eq!(v["summary"]["requests"], serde_json::json!({"total": 3, "passed": 2, "failed": 1}));
    assert_eq!(v["summary"]["assertions"], serde_json::json!({"total": 3, "passed": 2, "failed": 1}));
    let col = &v["collections"][0];
    assert_eq!(col["name"], "Run API");
    assert_eq!(col["environment"], "ci");
    let reqs = &col["iterations"][0]["requests"];
    assert_eq!(reqs[1]["name"], "Check & <compare>");
    assert_eq!(reqs[1]["status"], 200);
    assert!(reqs[1]["responseTimeMs"].is_u64() && reqs[1]["durationMs"].is_u64());
    assert_eq!(reqs[1]["failure"], "body contains \"yes\": not found in the response body");
    assert_eq!(reqs[1]["assertions"][1], serde_json::json!({"name": "body contains \"yes\"", "passed": false, "message": "not found in the response body"}));
    assert_eq!(reqs[0]["assertions"][0]["message"], Value::Null);
    assert_eq!(reqs[2]["url"], format!("{}/who/nobody", server.base_url()));
}

#[test]
fn secrets_are_masked_in_every_report() {
    let server = MockServer::start();
    let seen = server.mock(|when, then| {
        when.path("/ok").query_param("key", SECRET).header("authorization", format!("Bearer {SECRET}"));
        // The response echoes the secret; it fails assertion 200 → 201 below.
        then.status(201).header("x-echo", SECRET).body(SECRET);
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    fs::write(
        dir.join("requests/check.yaml"),
        "name: Check\nurl: ${base}/check\nscripts:\n  post: assert.equal(getVariable('token'), 'x', 'token is ' + getVariable('token'));\n",
    )
    .unwrap();
    // Once through --env with the env var, once through --env-file.
    let env_file = tmp.path().join("ci-file.yaml");
    fs::write(&env_file, format!("name: ci\nvariables:\n  base: {}\n  token:\n    secret: true\n", server.base_url())).unwrap();
    for (i, env_args) in [vec!["--env", "ci"], vec!["--env-file", env_file.to_str().unwrap()]].into_iter().enumerate() {
        let out_dir = format!("out{i}");
        let mut args = vec!["run", "./api"];
        args.extend(env_args);
        args.extend(["--reporter", "cli,junit,json,html", "--out", &out_dir]);
        let out = hitop(&home, tmp.path()).args(&args).env("HITOP_SECRET_CI_TOKEN", SECRET).assert().code(1).get_output().clone();
        let junit = fs::read_to_string(tmp.path().join(&out_dir).join("junit.xml")).unwrap();
        let json = fs::read_to_string(tmp.path().join(&out_dir).join("report.json")).unwrap();
        let html = fs::read_to_string(tmp.path().join(&out_dir).join("report.html")).unwrap();
        let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        for (what, s) in [("junit", &junit), ("json", &json), ("html", &html), ("log", &log)] {
            assert!(!s.contains(SECRET), "{what} holds the secret:\n{s}");
            assert!(s.contains("••••"), "{what} has no mask:\n{s}");
        }
        assert!(junit.contains("key=••••") && json.contains("key=••••") && html.contains("key=••••"), "{json}");
        assert!(json.contains("token is ••••") && html.contains("token is ••••"), "{json}");
    }
    seen.assert_calls(2);
}

#[test]
fn html_report_is_one_escaped_page() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/check");
        then.status(200).body("<script>alert('body')</script>");
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--reporter", "html", "--out", "r"])
        .assert()
        .code(1)
        .stderr(contains("report.html"));
    // html alone: no other file.
    let names: Vec<String> = fs::read_dir(tmp.path().join("r")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["report.html"]);
    let html = fs::read_to_string(tmp.path().join("r/report.html")).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>") && html.contains("default-src 'none'"));
    // The request name "Check & <compare>" is escaped; no response body.
    assert!(html.contains("Check &amp; &lt;compare&gt;"), "{html}");
    assert!(!html.contains("<compare>") && !html.contains("<script") && !html.contains("alert("), "{html}");
    assert!(html.contains("body contains &quot;yes&quot;"), "{html}");
    assert!(html.contains("api/requests/check.yaml"), "{html}");
}

#[test]
fn usage_and_config_errors_exit_2() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, "http://127.0.0.1:9");
    fs::write(tmp.path().join("bad.csv"), "a,b\n1\n").unwrap();
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["run", "./api", "--env", "staging"], "environment 'staging' not found"),
        (vec!["run", "missing-collection"], "not found"),
        (vec!["run", "./api", "--env", "ci"], "HITOP_SECRET_CI_TOKEN"),
        (vec!["run", "./api", "--var", "novalue"], "--var takes KEY=VALUE"),
        (vec!["run", "./api", "--data", "none.csv"], "none.csv"),
        (vec!["run", "./api", "--data", "bad.csv"], "line 2"),
        (vec!["run", "./api", "--env-file", "none.yaml"], "none.yaml"),
        (vec!["run", "./api", "--reporter", "pdf"], "invalid value"),
        (vec!["run", "./api", "--env", "ci", "--env-file", "x.yaml"], "cannot be used with"),
        (vec!["run", "./api", "--iterations", "2", "--data", "bad.csv"], "cannot be used with"),
        (vec!["run", "./api", "--timeout", "0"], "invalid value"),
    ];
    for (args, message) in cases {
        hitop(&home, tmp.path()).args(&args).assert().code(2).stderr(contains(message));
    }
    // The reports directory can't be made: a file is in the way.
    fs::write(tmp.path().join("blocked"), "").unwrap();
    hitop(&home, tmp.path())
        .args(["run", "./api", "--secret", "token=tok-123", "--reporter", "json", "--out", "blocked/x", "--var", "base=http://127.0.0.1:9"])
        .assert()
        .code(2)
        .stderr(contains("--out"));
}

#[test]
fn help_lists_the_exit_codes() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    hitop(&home, tmp.path())
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(contains("Exit codes:").and(contains("0  every request passed")).and(contains("2  usage or configuration error")));
}

#[test]
fn a_request_error_exits_1() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, "http://127.0.0.1:9");
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=tok-123"]).assert().code(1);
}

#[test]
fn var_overrides_the_environment_and_env_file_is_used() {
    let server = MockServer::start();
    let who = server.mock(|when, then| {
        when.path("/who/ada");
        then.status(200);
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, "http://127.0.0.1:9");
    // The env file points at the mock; the folder's own ci.yaml doesn't.
    let env_file = tmp.path().join("mock.yaml");
    fs::write(&env_file, format!("variables:\n  base: {}\n  token: t\n  user: env-user\n", server.base_url())).unwrap();
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env-file", "mock.yaml", "--var", "user=ada", "--reporter", "json"])
        .assert()
        .code(0);
    who.assert();
    let v = read_json(tmp.path().join("hitop-reports/report.json"));
    assert_eq!(v["collections"][0]["environment"], "mock");
}

#[test]
fn data_rows_drive_iterations_from_csv_and_json() {
    let server = MockServer::start();
    let ada = server.mock(|when, then| {
        when.path("/who/ada");
        then.status(200);
    });
    let bob = server.mock(|when, then| {
        when.path("/who/bob");
        then.status(200);
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    fs::write(tmp.path().join("users.csv"), "user,note\nada,\"first, one\"\nbob,second\n").unwrap();
    fs::write(tmp.path().join("users.json"), r#"[{"user": "ada"}, {"user": "bob", "n": 2}]"#).unwrap();
    for file in ["users.csv", "users.json"] {
        hitop(&home, tmp.path())
            .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--data", file, "--reporter", "cli,json,junit"])
            .assert()
            .code(0)
            .stdout(contains("Iteration 2 of 2").and(contains("6/6 requests succeeded")).and(contains("in 2 iterations")));
        let v = read_json(tmp.path().join("hitop-reports/report.json"));
        assert_eq!(v["summary"]["iterations"], 2);
        let its = v["collections"][0]["iterations"].as_array().unwrap();
        assert_eq!(its[0]["iteration"], 1);
        assert_eq!(its[1]["data"]["user"], "bob");
        let junit = fs::read_to_string(tmp.path().join("hitop-reports/junit.xml")).unwrap();
        assert!(junit.contains("name=\"Run API (iteration 2)\"") && junit.contains("<property name=\"data.user\" value=\"bob\"/>"), "{junit}");
    }
    let v = read_json(tmp.path().join("hitop-reports/report.json"));
    assert_eq!(v["collections"][0]["iterations"][1]["data"]["n"], "2");
    ada.assert_calls(2);
    bob.assert_calls(2);
}

#[test]
fn iterations_repeat_the_run() {
    let server = MockServer::start();
    let ok = server.mock(|when, then| {
        when.path("/ok");
        then.status(200);
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--iterations", "3"])
        .assert()
        .code(0)
        .stdout(contains("Iteration 3 of 3").and(contains("9/9 requests succeeded")));
    ok.assert_calls(3);
}

#[test]
fn bail_stops_at_the_first_failure_and_the_default_runs_on() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/ok");
        then.status(500);
    });
    let rest = server.mock(|when, then| {
        when.path("/check");
        then.status(200).body("yes");
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--iterations", "2", "--bail", "--reporter", "cli,json"])
        .assert()
        .code(1)
        .stdout(contains("0/1 requests succeeded").and(contains("stopped at the first failure")));
    rest.assert_calls(0);
    let v = read_json(tmp.path().join("hitop-reports/report.json"));
    assert_eq!(v["bailed"], true);
    assert_eq!(v["summary"]["iterations"], 1);
    // Without --bail every request runs.
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=tok-123"]).assert().code(1).stdout(contains("2/3 requests succeeded"));
    rest.assert_calls(1);
    // The old flag still parses.
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--no-stop-on-fail"]).assert().code(1);
}

#[test]
fn timeout_fails_a_slow_request() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/ok");
        then.status(200).delay(Duration::from_millis(1500));
    });
    all_ok(&server);
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123", "--timeout", "200", "--reporter", "cli,json"])
        .assert()
        .code(1)
        .stdout(contains("timed out").and(contains("2/3 requests succeeded")));
    let v = read_json(tmp.path().join("hitop-reports/report.json"));
    let r = &v["collections"][0]["iterations"][0]["requests"][0];
    assert!(r["error"].as_str().unwrap().contains("timed out"), "{r}");
    assert_eq!(r["status"], Value::Null);
}

#[test]
fn oauth2_client_credentials_fetches_one_token_for_the_run() {
    let server = MockServer::start();
    let token = server.mock(|when, then| {
        when.method(POST).path("/token").header("content-type", "application/x-www-form-urlencoded").body("grant_type=client_credentials&scope=orders");
        then.status(200).json_body(serde_json::json!({ "access_token": "oauth-tok-9", "token_type": "Bearer", "expires_in": 3600 }));
    });
    let api = server.mock(|when, then| {
        when.method(GET).header("authorization", "Bearer oauth-tok-9");
        then.status(200).body("yes");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    let manifest = fs::read_to_string(dir.join("hitop.yaml")).unwrap().replace(
        "auth:\n  type: bearer\n  token: ${token}\n",
        "auth:\n  type: oauth2\n  grant: client_credentials\n  tokenUrl: ${base}/token\n  clientId: app\n  clientSecret: ${token}\n  scope: orders\n",
    );
    fs::write(dir.join("hitop.yaml"), manifest).unwrap();
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--secret", "token=tok-123"])
        .assert()
        .code(0)
        .stdout(contains("3/3 requests succeeded"));
    token.assert_calls(1);
    api.assert_calls(3);
    // A refused client: the run says why, and nothing is sent.
    let refused = MockServer::start();
    refused.mock(|when, then| {
        when.method(POST).path("/token");
        then.status(401).json_body(serde_json::json!({ "error": "invalid_client" }));
    });
    let dir2 = TempDir::new().unwrap();
    let d2 = folder(&dir2, &refused.base_url());
    fs::write(d2.join("hitop.yaml"), fs::read_to_string(dir.join("hitop.yaml")).unwrap()).unwrap();
    hitop(&home, dir2.path()).args(["run", "./api", "--env", "ci", "--secret", "token=tok-123"]).assert().code(1).stdout(contains("invalid_client"));
}

#[test]
fn awsv4_signs_each_request_as_sent() {
    let server = MockServer::start();
    let api = server.mock(|when, then| {
        when.method(GET).path("/orders").header_exists("x-amz-date").header_exists("x-amz-content-sha256").header_matches("^authorization$", r"^AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/\d{8}/eu-west-1/execute-api/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=[0-9a-f]{64}$");
        then.status(200).body("yes");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Run API\nauth:\n  type: awsv4\n  accessKey: AKIDEXAMPLE\n  secretKey: ${token}\n  region: eu-west-1\n  service: execute-api\norder:\n- ok\n").unwrap();
    fs::write(dir.join("requests/ok.yaml"), "name: Ok\nurl: ${base}/orders\n").unwrap();
    fs::remove_file(dir.join("requests/check.yaml")).unwrap();
    fs::remove_file(dir.join("requests/who.yaml")).unwrap();
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=wJalrXUtnFEMI"]).assert().code(0);
    api.assert_calls(1);
}

#[test]
fn digest_answers_the_challenge_once() {
    let server = MockServer::start();
    let challenge = server.mock(|when, then| {
        when.method(GET).path("/orders").header_missing("authorization");
        then.status(401).header("WWW-Authenticate", r#"Digest realm="shop", qop="auth", nonce="n0nce42", opaque="op1""#);
    });
    let answered = server.mock(|when, then| {
        when.method(GET).path("/orders").header_matches("^authorization$", r#"^Digest username="ada", realm="shop", nonce="n0nce42", uri="/orders", algorithm=MD5, response="[0-9a-f]{32}", qop=auth, nc=00000001, cnonce="[0-9a-f]{16}", opaque="op1"$"#);
        then.status(200).body("yes");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Run API\nauth:\n  type: digest\n  username: ada\n  password: ${token}\norder:\n- ok\n").unwrap();
    fs::write(dir.join("requests/ok.yaml"), "name: Ok\nurl: ${base}/orders\nscripts:\n  post: assert.status(200);\n").unwrap();
    fs::remove_file(dir.join("requests/check.yaml")).unwrap();
    fs::remove_file(dir.join("requests/who.yaml")).unwrap();
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=Circle"]).assert().code(0);
    challenge.assert_calls(1);
    answered.assert_calls(1);
}

#[test]
fn oauth2_authorization_code_with_pkce_signs_in_once() {
    let server = MockServer::start();
    let token = server.mock(|when, then| {
        when.method(POST).path("/token").body_includes("grant_type=authorization_code").body_includes("code=c0de").body_includes("code_verifier=").body_includes("client_id=app");
        then.status(200).json_body(serde_json::json!({ "access_token": "user-tok-7", "expires_in": 3600 }));
    });
    let api = server.mock(|when, then| {
        when.method(GET).header("authorization", "Bearer user-tok-7");
        then.status(200).body("yes");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    // Plays the browser: checks the sign-in URL asks for PKCE, then follows the redirect with a code.
    let opener = tmp.path().join("opener.sh");
    fs::write(&opener, "#!/bin/sh\npython3 - \"$1\" <<'EOF'\nimport sys, urllib.parse as u, urllib.request as r\nq = u.parse_qs(u.urlparse(sys.argv[1]).query)\nassert q['code_challenge_method'] == ['S256'] and q['response_type'] == ['code'] and q['client_id'] == ['app']\nr.urlopen(q['redirect_uri'][0] + '?code=c0de&state=' + u.quote(q['state'][0])).read()\nEOF\n").unwrap();
    std::process::Command::new("chmod").arg("+x").arg(&opener).status().unwrap();
    let dir = folder(&tmp, &server.base_url());
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Run API\nvariables:\n  user: nobody\nauth:\n  type: oauth2\n  grant: authorization_code\n  authUrl: ${base}/authorize\n  tokenUrl: ${base}/token\n  clientId: app\n  scope: orders\norder:\n- ok\n- check\n- who\n").unwrap();
    hitop(&home, tmp.path()).env("HITOP_OAUTH_OPENER", &opener).args(["run", "./api", "--env", "ci", "--secret", "token=x"]).assert().code(0);
    token.assert_calls(1);
    api.assert_calls(3);
    // Without a person at the terminal (no opener, stderr not a terminal), it says so.
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=x"]).assert().code(1).stdout(contains("needs a person to sign in"));
}

#[test]
fn report_files_skip_websocket_and_grpc_requests() {
    // A run skips WebSocket and gRPC requests; each step's `file` is still its own request's.
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/orders");
        then.status(200).body("yes");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let dir = folder(&tmp, &server.base_url());
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Run API\norder:\n- socket\n- ok\n").unwrap();
    fs::write(dir.join("requests/socket.yaml"), "name: Socket\nkind: websocket\nurl: ws://127.0.0.1:9/feed\n").unwrap();
    fs::write(dir.join("requests/ok.yaml"), "name: Ok\nurl: ${base}/orders\n").unwrap();
    fs::remove_file(dir.join("requests/check.yaml")).unwrap();
    fs::remove_file(dir.join("requests/who.yaml")).unwrap();
    hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci", "--secret", "token=t", "--reporter", "json", "--out", "r"]).assert().code(0);
    let v: Value = serde_json::from_str(&fs::read_to_string(tmp.path().join("r/report.json")).unwrap()).unwrap();
    let steps = v["collections"][0]["iterations"][0]["requests"].as_array().unwrap();
    assert_eq!(steps.len(), 1, "{v}");
    assert_eq!((steps[0]["name"].as_str(), steps[0]["file"].as_str()), (Some("Ok"), Some("api/requests/ok.yaml")), "{v}");
}
