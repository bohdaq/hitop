//! `hitop import` of real Postman collections (cli/fixtures/postman: Conduit,
//! newman's sample, Adyen's BIN lookup) into collection folders, then
//! `hitop run` against a local mock server. The keychain is off, so secret
//! values come from HITOP_SECRET_* env vars; nothing reaches the internet.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::{json, Value};
use tempfile::TempDir;

fn hitop(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .current_dir(home.path());
    c
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/postman").join(name)
}

fn arg(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Runs the folder with the JSON reporter; returns the exit code and report.
fn run(home: &TempDir, dir: &Path, extra: &[&str], envs: &[(&str, &str)]) -> (i32, Value) {
    let out = home.path().join("reports");
    let mut c = hitop(home);
    c.args(["run", arg(dir), "--reporter", "cli,json", "--out", arg(&out)]).args(extra);
    for (k, v) in envs {
        c.env(k, v);
    }
    let output = c.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let report: Value = serde_json::from_str(&fs::read_to_string(out.join("report.json")).unwrap_or_else(|e| panic!("{e}\n{stdout}"))).unwrap();
    (output.status.code().unwrap_or(-1), report)
}

fn failures(report: &Value) -> Vec<String> {
    let mut out = vec![];
    for c in report["collections"].as_array().unwrap() {
        for it in c["iterations"].as_array().unwrap() {
            for r in it["requests"].as_array().unwrap() {
                let name = r["name"].as_str().unwrap_or_default();
                let failed: Vec<&Value> = r["assertions"].as_array().unwrap().iter().filter(|a| a["passed"] != true).collect();
                if r["passed"] != true && failed.is_empty() {
                    out.push(format!("{name}: status {} {}", r["status"], r["error"]));
                }
                for a in failed {
                    out.push(format!("{name} / {}: {}", a["name"].as_str().unwrap_or_default(), a["message"].as_str().unwrap_or_default()));
                }
            }
        }
    }
    out
}

fn assertions(report: &Value) -> u64 {
    report["summary"]["assertions"]["total"].as_u64().unwrap()
}

/// Every request file under `dir/requests`, with `from` replaced by `to`.
fn rewrite(dir: &Path, from: &str, to: &str) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rewrite(&path, from, to);
        } else {
            let text = fs::read_to_string(&path).unwrap();
            fs::write(&path, text.replace(from, to)).unwrap();
        }
    }
}

fn json_mock(server: &MockServer, method: httpmock::Method, path: &str, body: Value) {
    server.mock(|when, then| {
        when.method(method).path(path);
        then.status(200).header("content-type", "application/json; charset=utf-8").json_body(body);
    });
}

const TOKEN: &str = "jwt.token.42";
const SLUG: &str = "how-to-train-your-dragon-42";

/// A Conduit (RealWorld) API good enough for the collection's checks. Every
/// route that needs the token from Login only answers when it is sent, and
/// the article routes only answer on the slug Create Article returned, so a
/// pass proves `pm.globals.set` reaches later requests.
fn conduit(server: &MockServer) {
    let ts = "2026-11-01T10:00:00.000Z";
    let user = json!({"user": {"email": "hitop@example.com", "username": "hitop", "bio": null, "image": null, "token": TOKEN}});
    let author = json!({"username": "hitop", "bio": null, "image": null, "following": false});
    let article = |favorited: bool, count: u64| {
        json!({"slug": SLUG, "title": "How to train your dragon", "description": "Ever wonder how?", "body": "Very carefully.",
               "tagList": ["dragons", "training"], "createdAt": ts, "updatedAt": ts, "favorited": favorited, "favoritesCount": count, "author": author})
    };
    let comment = json!({"id": 7, "body": "Thank you so much!", "createdAt": ts, "updatedAt": ts, "author": author});
    let profile = |following: bool| json!({"profile": {"username": "celeb_hitop", "bio": null, "image": null, "following": following}});
    let auth = format!("Token {TOKEN}");
    let with_token = |method: httpmock::Method, path: String, body: Value| {
        let auth = auth.clone();
        server.mock(move |when, then| {
            when.method(method).path(path).header("authorization", auth);
            then.status(200).header("content-type", "application/json").json_body(body);
        });
    };
    json_mock(server, POST, "/api/users", user.clone());
    // Login must send the secret password from the environment.
    server.mock(|when, then| {
        when.method(POST).path("/api/users/login").body_includes("dummy-password-1");
        then.status(200).header("content-type", "application/json").json_body(user.clone());
    });
    with_token(GET, "/api/user".into(), user.clone());
    with_token(PUT, "/api/user".into(), user.clone());
    json_mock(server, GET, "/api/articles", json!({"articles": [article(true, 1)], "articlesCount": 1}));
    with_token(GET, "/api/articles/feed".into(), json!({"articles": [], "articlesCount": 0}));
    with_token(POST, "/api/articles".into(), json!({"article": article(false, 0)}));
    for m in [GET, PUT, DELETE] {
        with_token(m, format!("/api/articles/{SLUG}"), json!({"article": article(false, 0)}));
    }
    with_token(POST, format!("/api/articles/{SLUG}/favorite"), json!({"article": article(true, 1)}));
    with_token(DELETE, format!("/api/articles/{SLUG}/favorite"), json!({"article": article(false, 0)}));
    with_token(POST, format!("/api/articles/{SLUG}/comments"), json!({"comment": comment}));
    json_mock(server, GET, &format!("/api/articles/{SLUG}/comments"), json!({"comments": [comment]}));
    with_token(DELETE, format!("/api/articles/{SLUG}/comments/7"), json!({}));
    with_token(GET, "/api/profiles/celeb_hitop".into(), profile(false));
    with_token(POST, "/api/profiles/celeb_hitop/follow".into(), profile(true));
    with_token(DELETE, "/api/profiles/celeb_hitop/follow".into(), profile(false));
    json_mock(server, GET, "/api/tags", json!({"tags": ["dragons", "training"]}));
}

#[test]
fn conduit_imports_with_its_environment_and_passes() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("conduit");
    let f = fixture("conduit");
    hitop(&home)
        .args(["import", arg(&f.join("collection.json")), arg(&f.join("conduit.postman_environment.json")), "--to", arg(&dir)])
        .assert()
        .success()
        .stdout(contains("Imported 'Conduit'").and(contains("(32 requests)")).and(contains("Imported environment 'Conduit local'")))
        .stderr(contains("secret 'PASSWORD' of 'Conduit local' not stored").and(contains("HITOP_SECRET_CONDUIT_LOCAL_PASSWORD")));
    assert!(dir.join("requests/auth/login-and-remember-token.yaml").is_file());
    assert!(dir.join("requests/articles-favorite-comments/all-articles.yaml").is_file(), "folders become directories");
    let env = fs::read_to_string(dir.join("environments/conduit-local.yaml")).unwrap();
    assert!(env.contains("PASSWORD:\n    secret: true"), "{env}");
    assert!(!env.contains("dummy-password-1"), "secret values never go into files");

    let server = MockServer::start();
    conduit(&server);
    let api = server.url("/api");
    let (code, report) = run(&home, &dir, &["--env", "Conduit local", "--var", &format!("APIURL={api}")], &[("HITOP_SECRET_CONDUIT_LOCAL_PASSWORD", "dummy-password-1")]);
    assert_eq!(failures(&report), Vec::<String>::new());
    assert_eq!(code, 0);
    assert_eq!(report["summary"]["requests"]["total"], 32);
    assert!(assertions(&report) > 300, "the legacy tests[...] checks ran: {}", assertions(&report));
    assert!(!report.to_string().contains("dummy-password-1"), "the secret is masked in reports");
}

#[test]
fn newman_sample_passes_with_pm_test_and_expect() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("newman");
    hitop(&home).args(["import", arg(&fixture("newman-sample").join("collection.json")), "--to", arg(&dir)]).assert().success();
    // The sample calls postman-echo.com; point it at the mock instead.
    let server = MockServer::start();
    rewrite(&dir.join("requests"), "https://postman-echo.com", &server.base_url());
    server.mock(|when, then| {
        when.method(GET).path("/get").query_param("source", "newman-sample-github-collection");
        then.status(200).header("content-type", "application/json").json_body(json!({"args": {"source": "newman-sample-github-collection"}}));
    });
    server.mock(|when, then| {
        when.method(POST).path("/post");
        then.status(200).header("content-type", "application/json").json_body(json!({"data": "ok"}));
    });
    let (code, report) = run(&home, &dir, &[], &[]);
    assert_eq!(failures(&report), Vec::<String>::new());
    assert_eq!(code, 0);
    assert_eq!(assertions(&report), 2);

    // The same checks fail, with chai's message, on a wrong answer.
    let wrong = MockServer::start();
    rewrite(&dir.join("requests"), &server.base_url(), &wrong.base_url());
    wrong.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(json!({"args": {"source": "other"}}));
    });
    let (code, report) = run(&home, &dir, &[], &[]);
    assert_eq!(code, 1);
    assert_eq!(failures(&report), vec!["A simple GET request / expect response json contain args: expected 'other' to equal 'newman-sample-github-collection'"]);
}

#[test]
fn adyen_imports_folders_auth_and_secret_globals() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("adyen");
    let f = fixture("adyen-binlookup");
    hitop(&home)
        .args(["import", arg(&f.join("collection.json")), arg(&f.join("adyen.postman_globals.json")), "--to", arg(&dir)])
        .assert()
        .success()
        .stdout(contains("Imported environment 'Adyen test globals'").and(contains("thing").not()));
    assert!(dir.join("requests/general/get3dsavailability/get-3d-secure-availability.yaml").is_file());
    let manifest = fs::read_to_string(dir.join("hitop.yaml")).unwrap();
    assert!(manifest.contains("auth:\n  type: apikey\n  key: X-API-Key\n  value: ${X-API-Key}"), "{manifest}");
    assert!(!fs::read_to_string(dir.join("environments/adyen-test-globals.yaml")).unwrap().contains("dummy-api-key-2"));

    let server = MockServer::start();
    let calls = server.mock(|when, then| {
        when.method(POST).header("x-api-key", "dummy-api-key-2").body_includes("\"merchantAccount\": \"HitopTest\"");
        then.status(200).json_body(json!({"threeDS1Supported": true}));
    });
    let (code, report) = run(
        &home,
        &dir,
        &["--env", "Adyen test globals", "--var", &format!("baseUrl={}", server.base_url())],
        &[("HITOP_SECRET_ADYEN_TEST_GLOBALS_X_API_KEY", "dummy-api-key-2")],
    );
    assert_eq!(failures(&report), Vec::<String>::new());
    assert_eq!(code, 0);
    assert_eq!(report["summary"]["requests"]["total"], 6);
    calls.assert_calls(6);
}

#[test]
fn the_report_lists_unsupported_calls_by_line() {
    let home = TempDir::new().unwrap();
    let file = home.path().join("c.json");
    fs::write(
        &file,
        json!({
            "info": {"name": "Report", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [{"name": "Get", "request": {"method": "GET", "url": "{{base}}/x", "auth": {"type": "oauth2"}},
                      "event": [{"listen": "test", "script": {"exec": ["pm.test('ok', () => {});", "pm.sendRequest('{{base}}/y', () => {});", "postman.setNextRequest(null);"]}}]}]
        })
        .to_string(),
    )
    .unwrap();
    hitop(&home)
        .args(["import", arg(&file)])
        .assert()
        .success()
        .stdout(
            contains("'Report': 3 things to check (2 script lines with unsupported calls)")
                .and(contains("Get (post-request script) line 2: pm.sendRequest in `pm.sendRequest('{{base}}/y', () => {});`"))
                .and(contains("Get (post-request script) line 3: postman.setNextRequest"))
                .and(contains("Get (auth): oauth2 auth")),
        );
    // Imported into global storage, with ${base}.
    hitop(&home).args(["requests", "show", "Report", "Get"]).assert().success().stdout(contains("${base}/x"));
}

#[test]
fn env_import_keeps_postman_secrets_out_of_storage() {
    let home = TempDir::new().unwrap();
    hitop(&home)
        .args(["env", "import", arg(&fixture("conduit").join("conduit.postman_environment.json"))])
        .assert()
        .success()
        .stderr(contains("HITOP_SECRET_CONDUIT_LOCAL_PASSWORD"));
    let stored = fs::read_to_string(home.path().join("config/hitop/environments.json")).unwrap();
    assert!(stored.contains("\"PASSWORD\"") && !stored.contains("dummy-password-1"), "{stored}");
}
