//! The usage ping (`hitop telemetry`): nothing is sent while off, and on it
//! sends at most once a week, exactly id, surface, version and OS. The ping
//! goes to a local mock server (HITOP_TELEMETRY_URL), never the real one.
#![cfg(feature = "telemetry")]
use std::fs;

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn hitop(home: &TempDir, endpoint: &str) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .env("HITOP_TELEMETRY_URL", endpoint)
        .current_dir(home.path());
    c
}

fn settings(home: &TempDir) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(home.path().join("config/hitop/telemetry.json")).unwrap()).unwrap()
}

#[test]
fn nothing_is_sent_while_off() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200).body(r#"{"counted":true}"#);
    });
    let url = server.url("/api/ping");
    for args in [vec!["collections", "list"], vec!["env", "list"], vec!["telemetry", "status"]] {
        hitop(&home, &url).args(args).assert().success();
    }
    hitop(&home, &url)
        .args(["telemetry", "status"])
        .assert()
        .success()
        .stdout(contains("Usage ping: off (nothing is sent)").and(contains(format!("POST {url}"))).and(contains(r#""surface":"cli""#)));
    assert!(!home.path().join("config/hitop/telemetry.json").exists(), "off writes nothing");
    // On, then off again: still nothing, and the id is gone.
    hitop(&home, &url).args(["telemetry", "on"]).assert().success();
    hitop(&home, &url).args(["telemetry", "off"]).assert().success();
    hitop(&home, &url).args(["collections", "list"]).assert().success();
    assert!(settings(&home)["cli"].get("id").is_none());
    any.assert_calls(0);
}

#[test]
fn on_sends_at_most_once_a_week() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    hitop(&home, &server.url("/api/ping")).args(["telemetry", "on"]).assert().success().stdout(contains("Usage ping on"));
    let id = settings(&home)["cli"]["id"].as_str().unwrap().to_string();
    let os = match std::env::consts::OS {
        "macos" | "linux" | "windows" => std::env::consts::OS,
        _ => "other",
    };
    let ping = server.mock(|when, then| {
        when.method(POST)
            .path("/api/ping")
            .header("content-type", "application/json")
            .header_missing("origin")
            .header_missing("cookie")
            .json_body(serde_json::json!({ "id": id, "surface": "cli", "version": env!("CARGO_PKG_VERSION"), "os": os }));
        then.status(200).body(r#"{"counted":true}"#);
    });
    let url = server.url("/api/ping");
    // `telemetry` itself doesn't ping; the next command does, once.
    hitop(&home, &url).args(["telemetry", "status"]).assert().success().stdout(contains(&id).and(contains("Not sent yet")));
    ping.assert_calls(0);
    hitop(&home, &url).args(["collections", "list"]).assert().success();
    ping.assert_calls(1);
    for _ in 0..3 {
        hitop(&home, &url).args(["env", "list"]).assert().success();
    }
    ping.assert_calls(1);
    hitop(&home, &url).args(["telemetry", "status"]).assert().success().stdout(contains("Last sent"));

    // Eight days later it is due again.
    let mut s = settings(&home);
    let last = s["cli"]["lastPing"].as_u64().unwrap();
    s["cli"]["lastPing"] = serde_json::json!(last - 8 * 86_400);
    fs::write(home.path().join("config/hitop/telemetry.json"), s.to_string()).unwrap();
    hitop(&home, &url).args(["collections", "list"]).assert().success();
    hitop(&home, &url).args(["collections", "list"]).assert().success();
    ping.assert_calls(2);

    // A failed ping counts as this week's: no retry on every command.
    let mut s = settings(&home);
    s["cli"]["lastPing"] = serde_json::json!(last - 8 * 86_400);
    fs::write(home.path().join("config/hitop/telemetry.json"), s.to_string()).unwrap();
    let dead = "http://127.0.0.1:9/api/ping";
    hitop(&home, dead).args(["collections", "list"]).assert().success();
    hitop(&home, &url).args(["collections", "list"]).assert().success();
    ping.assert_calls(2);
}
