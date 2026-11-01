//! Collection folders from the command line: `init`, `convert`, and
//! commands that take a folder path instead of a collection name. HTTP goes
//! to a local mock server only.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// hitop with an isolated config dir, run in `cwd`.
fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).current_dir(cwd);
    c
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

/// `cli/fixtures/sample` copied to `<tmp>/fixtures/sample`, with an
/// environment pointing at the mock server.
fn sample_in(tmp: &TempDir, base: &str) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample");
    let dir = tmp.path().join("fixtures/sample");
    copy_dir(&fixture, &dir);
    fs::write(dir.join("environments/mock.yaml"), format!("variables:\n  base: {base}\n  token: test-token\n  apiKey: test-key\n")).unwrap();
    dir
}

#[test]
fn run_sample_folder_against_a_mock_server() {
    let server = MockServer::start();
    let user = server.mock(|when, then| {
        when.method(GET)
            .path("/users/1")
            .query_param("fields", "name,email")
            .header("authorization", "Bearer test-token")
            .header("accept", "application/json")
            // Rows switched off in the file are not sent.
            .header_missing("x-debug")
            .query_param_missing("debug");
        then.status(200).json_body(serde_json::json!({ "id": 1, "name": "Ada" }));
    });
    let create = server.mock(|when, then| {
        when.method(POST)
            .path("/orders")
            .header("authorization", "Bearer test-token")
            .json_body(serde_json::json!({ "product": 42, "quantity": 1 }));
        then.status(201).json_body(serde_json::json!({ "id": 42 }));
    });
    let get = server.mock(|when, then| {
        // The request's own auth replaces the collection's bearer token.
        when.method(GET).path("/orders/42").header("x-api-key", "test-key").header_missing("authorization");
        then.status(200).json_body(serde_json::json!({ "id": 42, "status": "paid" }));
    });

    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    sample_in(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./fixtures/sample", "--env", "mock"])
        .assert()
        .success()
        .stdout(contains("Sample API").and(contains("3/3 requests succeeded")));
    user.assert();
    create.assert();
    get.assert();
}

#[test]
fn failing_assertion_in_a_folder_exits_1() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.any_request();
        then.status(200).json_body(serde_json::json!({ "name": "Bob", "id": 1 }));
    });
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    sample_in(&tmp, &server.base_url());
    hitop(&home, tmp.path()).args(["run", "./fixtures/sample", "--env", "mock"]).assert().code(1);
}

#[test]
fn unknown_folder_environment_is_an_error() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    sample_in(&tmp, "http://127.0.0.1:9");
    hitop(&home, tmp.path())
        .args(["run", "./fixtures/sample", "--env", "staging"])
        .assert()
        .failure()
        .stderr(contains("environment 'staging' not found").and(contains("local, mock, prod")));
}

#[test]
fn path_without_hitop_yaml_says_so() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("empty")).unwrap();
    hitop(&home, tmp.path()).args(["run", "./empty"]).assert().failure().stderr(contains("no hitop.yaml"));
}

#[test]
fn init_creates_a_folder_and_refuses_twice() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    hitop(&home, tmp.path()).args(["init", "my-api"]).assert().success().stdout(contains("Created collection 'my-api'"));
    for f in ["hitop.yaml", "requests/get-example.yaml", "environments/local.yaml"] {
        assert!(tmp.path().join("my-api").join(f).is_file(), "{f}");
    }
    hitop(&home, tmp.path()).args(["requests", "list", "./my-api"]).assert().success().stdout(contains("Get example"));
    hitop(&home, tmp.path()).args(["init", "my-api"]).assert().failure().stderr(contains("already has hitop.yaml"));
}

#[test]
fn convert_json_to_folder_and_back() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let json = serde_json::json!([{
        "id": 5, "name": "Billing", "variables": { "base": "https://billing.test" },
        "requests": [
            { "id": 6, "name": "Create invoice", "method": "POST", "url": "${base}/invoices?draft=1",
              "headers": [{ "name": "Content-Type", "value": "application/json" }],
              "body": "{\n  \"amount\": 10\n}", "preRequestScript": "", "postRequestScript": "assert.status(201);",
              "bodyMode": "raw", "disabledParams": [{ "key": "trace", "value": "1" }] }
        ]
    }]);
    fs::write(tmp.path().join("billing.json"), serde_json::to_string(&json).unwrap()).unwrap();
    let envs = serde_json::json!({ "active": null, "environments": [{ "id": "e1", "name": "dev", "variables": { "base": "http://localhost" } }] });
    fs::write(tmp.path().join("envs.json"), envs.to_string()).unwrap();

    hitop(&home, tmp.path())
        .args(["convert", "billing.json", "billing", "--environments", "envs.json"])
        .assert()
        .success()
        .stdout(contains("Wrote 'Billing' (1 requests, 1 environments)"));
    let yaml = fs::read_to_string(tmp.path().join("billing/requests/create-invoice.yaml")).unwrap();
    assert!(yaml.contains("url: ${base}/invoices\n") && yaml.contains("content: |-\n    {\n"), "{yaml}");
    assert!(tmp.path().join("billing/environments/dev.yaml").is_file());

    hitop(&home, tmp.path()).args(["convert", "billing.json", "billing"]).assert().failure().stderr(contains("--force"));
    hitop(&home, tmp.path())
        .args(["convert", "./billing", "back.json", "--environments", "back-envs.json"])
        .assert()
        .success();
    let back: serde_json::Value = serde_json::from_str(&fs::read_to_string(tmp.path().join("back.json")).unwrap()).unwrap();
    let mut want = json.clone();
    want[0]["requests"][0].as_object_mut().unwrap().remove("bodyMode"); // "raw" is the default
    assert_eq!(back, want);
    let back_envs: serde_json::Value = serde_json::from_str(&fs::read_to_string(tmp.path().join("back-envs.json")).unwrap()).unwrap();
    assert_eq!(back_envs, envs);
}

#[test]
fn convert_needs_a_pick_when_the_json_has_several_collections() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let two = r#"[{"id":1,"name":"A","requests":[]},{"id":2,"name":"B","requests":[]}]"#;
    fs::write(tmp.path().join("two.json"), two).unwrap();
    hitop(&home, tmp.path()).args(["convert", "two.json", "out"]).assert().failure().stderr(contains("--collection"));
    hitop(&home, tmp.path()).args(["convert", "two.json", "out", "-c", "B"]).assert().success();
    assert!(fs::read_to_string(tmp.path().join("out/hitop.yaml")).unwrap().contains("name: B\n"));
}

#[test]
fn edits_through_a_path_keep_unknown_keys() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    hitop(&home, tmp.path()).args(["init", "api"]).assert().success();
    let manifest = tmp.path().join("api/hitop.yaml");
    fs::write(&manifest, fs::read_to_string(&manifest).unwrap() + "owner: team-a\n").unwrap();

    hitop(&home, tmp.path())
        .args(["requests", "add", "./api", "--name", "List orders", "--url", "${base}/orders", "-q", "page=2"])
        .assert()
        .success();
    hitop(&home, tmp.path()).args(["variables", "set", "./api", "region", "eu"]).assert().success();
    let text = fs::read_to_string(&manifest).unwrap();
    assert!(text.contains("owner: team-a\n") && text.contains("region: eu\n"), "{text}");
    let added = fs::read_to_string(tmp.path().join("api/requests/list-orders.yaml")).unwrap();
    assert!(added.contains("params:\n- name: page\n  value: '2'\n"), "{added}");

    hitop(&home, tmp.path()).args(["requests", "delete", "./api", "List orders"]).assert().success();
    assert!(!tmp.path().join("api/requests/list-orders.yaml").exists());
    // Nothing went to global storage.
    hitop(&home, tmp.path()).args(["collections", "list"]).assert().success().stdout(contains("No collections yet"));
}

#[test]
fn send_and_export_take_a_folder() {
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET).path("/ping");
        then.status(200).body("pong");
    });
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    let dir = sample_in(&tmp, &server.base_url());
    let dir = dir.to_str().unwrap();
    hitop(&home, tmp.path())
        .args(["send", "${base}/ping", "-c", dir, "--env", "mock", "--raw"])
        .assert()
        .success()
        .stdout(contains("pong"));
    m.assert();
    hitop(&home, tmp.path())
        .args(["export", "./fixtures/sample", "--format", "postman"])
        .assert()
        .success()
        .stdout(contains("Create order"));
}
