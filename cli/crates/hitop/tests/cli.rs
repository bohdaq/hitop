use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// Build a Command that uses an isolated config dir so tests never touch
/// ~/.config/hitop or ~/Library/Application Support/hitop.
fn cmd(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    // dirs uses $HOME on all platforms for its base.
    // On macOS: config_dir = $HOME/Library/Application Support
    // On Linux: config_dir = $XDG_CONFIG_HOME or $HOME/.config
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"));
    c
}

// ---------------------------------------------------------------------------
// Help / version
// ---------------------------------------------------------------------------

#[test]
fn help_exits_zero() {
    Command::cargo_bin("hitop").unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("send"))
        .stdout(contains("run"))
        .stdout(contains("collections"))
        .stdout(contains("history"));
}

#[test]
fn version_exits_zero() {
    Command::cargo_bin("hitop").unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn send_help_shows_url_and_method() {
    Command::cargo_bin("hitop").unwrap()
        .args(["send", "--help"])
        .assert()
        .success()
        .stdout(contains("URL"))
        .stdout(contains("method"));
}

#[test]
fn run_help_shows_collection_arg() {
    Command::cargo_bin("hitop").unwrap()
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(contains("collection"));
}

// ---------------------------------------------------------------------------
// Collections CRUD
// ---------------------------------------------------------------------------

#[test]
fn collections_list_empty() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("No collections"));
}

#[test]
fn collections_add_creates_collection() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["collections", "add", "MyAPI"])
        .assert()
        .success()
        .stdout(contains("MyAPI"));

    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("MyAPI"));
}

#[test]
fn collections_add_duplicate_fails() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "MyAPI"]).assert().success();
    cmd(&home)
        .args(["collections", "add", "MyAPI"])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn collections_delete_removes_collection() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "ToDelete"]).assert().success();
    cmd(&home).args(["collections", "delete", "ToDelete"]).assert().success();
    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("No collections"));
}

#[test]
fn collections_delete_nonexistent_fails() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["collections", "delete", "Ghost"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn collections_rename() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "Old"]).assert().success();
    cmd(&home).args(["collections", "rename", "Old", "New"]).assert().success();
    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("New"));
}

#[test]
fn collections_duplicate() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "Original"]).assert().success();
    cmd(&home).args(["collections", "duplicate", "Original"]).assert().success();
    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("Original"))
        .stdout(contains("Copy"));
}

// ---------------------------------------------------------------------------
// Requests CRUD
// ---------------------------------------------------------------------------

#[test]
fn requests_list_empty() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["requests", "list", "API"])
        .assert()
        .success()
        .stdout(contains("No requests"));
}

#[test]
fn requests_add_and_list() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["requests", "add", "API",
               "--name", "Get todos",
               "--url", "https://example.com/todos",
               "--method", "GET"])
        .assert()
        .success();

    cmd(&home)
        .args(["requests", "list", "API"])
        .assert()
        .success()
        .stdout(contains("Get todos"))
        .stdout(contains("https://example.com/todos"));
}

#[test]
fn requests_delete() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["requests", "add", "API", "--name", "Temp", "--url", "https://x.com"])
        .assert().success();
    cmd(&home).args(["requests", "delete", "API", "Temp"]).assert().success();
    cmd(&home)
        .args(["requests", "list", "API"])
        .assert()
        .success()
        .stdout(contains("No requests"));
}

#[test]
fn requests_show_displays_details() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["requests", "add", "API",
               "--name", "Get user",
               "--url", "https://api.example.com/user",
               "--method", "GET"])
        .assert().success();

    cmd(&home)
        .args(["requests", "show", "API", "Get user"])
        .assert()
        .success()
        .stdout(contains("Get user"))
        .stdout(contains("https://api.example.com/user"))
        .stdout(contains("GET"));
}

#[test]
fn requests_rename() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["requests", "add", "API", "--name", "OldName", "--url", "https://x.com"])
        .assert().success();
    cmd(&home)
        .args(["requests", "rename", "API", "OldName", "NewName"])
        .assert().success();
    cmd(&home)
        .args(["requests", "list", "API"])
        .assert()
        .success()
        .stdout(contains("NewName"));
}

#[test]
fn requests_add_to_nonexistent_collection_fails() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["requests", "add", "NoSuchCollection", "--name", "r", "--url", "https://x.com"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

// ---------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------

#[test]
fn variables_list_empty() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["variables", "list", "API"])
        .assert()
        .success()
        .stdout(contains("No variables"));
}

#[test]
fn variables_set_and_list() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["variables", "set", "API", "hostname", "https://api.example.com"])
        .assert().success();

    cmd(&home)
        .args(["variables", "list", "API"])
        .assert()
        .success()
        .stdout(contains("hostname"))
        .stdout(contains("https://api.example.com"));
}

#[test]
fn variables_delete() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home).args(["variables", "set", "API", "key", "val"]).assert().success();
    cmd(&home).args(["variables", "delete", "API", "key"]).assert().success();
    cmd(&home)
        .args(["variables", "list", "API"])
        .assert()
        .success()
        .stdout(contains("No variables"));
}

#[test]
fn variables_delete_nonexistent_fails() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "API"]).assert().success();
    cmd(&home)
        .args(["variables", "delete", "API", "ghost"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[test]
fn history_empty() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["history"])
        .assert()
        .success()
        .stdout(contains("No history"));
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

#[test]
fn import_hitop_format() {
    let home = TempDir::new().unwrap();
    let json = r#"[{"id":1,"name":"Imported","requests":[],"variables":{}}]"#;
    let file = home.path().join("col.json");
    std::fs::write(&file, json).unwrap();

    cmd(&home)
        .args(["import", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("Imported"));

    cmd(&home)
        .args(["collections", "list"])
        .assert()
        .success()
        .stdout(contains("Imported"));
}

#[test]
fn export_hitop_format_to_stdout() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "MyCol"]).assert().success();
    cmd(&home)
        .args(["export", "MyCol", "--format", "hitop"])
        .assert()
        .success()
        .stdout(contains("MyCol"));
}

#[test]
fn export_postman_format_to_stdout() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["collections", "add", "MyCol"]).assert().success();
    cmd(&home)
        .args(["export", "MyCol", "--format", "postman"])
        .assert()
        .success()
        .stdout(contains("getpostman.com"));
}

#[test]
fn export_nonexistent_collection_fails() {
    let home = TempDir::new().unwrap();
    cmd(&home)
        .args(["export", "NoSuch"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn import_then_export_roundtrip() {
    let home = TempDir::new().unwrap();
    let json = serde_json::json!([{
        "id": 99,
        "name": "RoundTrip",
        "requests": [{
            "id": 1,
            "name": "ping",
            "url": "https://example.com",
            "method": "GET",
            "headers": [],
            "body": ""
        }],
        "variables": {"host": "https://example.com"}
    }])
    .to_string();
    let file = home.path().join("rt.json");
    std::fs::write(&file, &json).unwrap();

    cmd(&home).args(["import", file.to_str().unwrap()]).assert().success();

    cmd(&home)
        .args(["export", "RoundTrip"])
        .assert()
        .success()
        .stdout(contains("RoundTrip"))
        .stdout(contains("ping"))
        .stdout(contains("https://example.com"));
}

// ---------------------------------------------------------------------------
// Shell completions
// ---------------------------------------------------------------------------

#[test]
fn completions_bash() {
    Command::cargo_bin("hitop").unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("hitop"));
}

#[test]
fn completions_zsh() {
    Command::cargo_bin("hitop").unwrap()
        .args(["completions", "zsh"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Environments, auth, query parameters and assertions
// ---------------------------------------------------------------------------

/// A one-thread HTTP server on 127.0.0.1 that answers every request with a
/// JSON echo of its request line and headers. Returns the base URL.
fn echo_server() -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let target = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let mut headers = serde_json::Map::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).unwrap() == 0 || h == "\r\n" {
                    break;
                }
                if let Some((k, v)) = h.trim_end().split_once(':') {
                    headers.insert(k.to_lowercase(), serde_json::Value::String(v.trim().to_string()));
                }
            }
            let body = serde_json::json!({ "target": target, "headers": headers }).to_string();
            let mut out = stream;
            let _ = write!(out, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    format!("http://{addr}")
}

#[test]
fn env_lifecycle() {
    let home = TempDir::new().unwrap();
    cmd(&home).args(["env", "add", "staging"]).assert().success();
    cmd(&home).args(["env", "set", "staging", "token", "stg"]).assert().success();
    cmd(&home).args(["env", "use", "staging"]).assert().success();
    cmd(&home).args(["env", "list"]).assert().success().stdout(contains("staging")).stdout(contains("1 variables"));
    cmd(&home).args(["env", "show", "staging"]).assert().success().stdout(contains("stg"));
    cmd(&home).args(["env", "add", "staging"]).assert().failure();
    cmd(&home).args(["env", "off"]).assert().success();
    cmd(&home).args(["env", "delete", "staging"]).assert().success();
    cmd(&home).args(["env", "show", "staging"]).assert().failure();
}

#[test]
fn env_imports_postman_and_exports() {
    let home = TempDir::new().unwrap();
    let file = home.path().join("pm.json");
    std::fs::write(&file, r#"{"name":"Prod","values":[{"key":"host","value":"https://api","enabled":true},{"key":"off","value":"x","enabled":false}]}"#).unwrap();
    cmd(&home).args(["env", "import", file.to_str().unwrap()]).assert().success().stdout(contains("Imported 1 environment"));
    cmd(&home).args(["env", "show", "Prod"]).assert().success().stdout(contains("https://api")).stdout(contains("off").not());
    let out = home.path().join("out.json");
    cmd(&home).args(["env", "export", out.to_str().unwrap()]).assert().success();
    assert!(std::fs::read_to_string(out).unwrap().contains("\"host\""));
}

#[test]
fn send_applies_env_auth_and_query() {
    let home = TempDir::new().unwrap();
    let base = echo_server();
    cmd(&home).args(["env", "add", "dev"]).assert().success();
    cmd(&home).args(["env", "set", "dev", "token", "dev-123"]).assert().success();
    cmd(&home).args(["env", "use", "dev"]).assert().success();
    cmd(&home)
        .args(["send", &format!("{base}/x?a=1"), "-q", "b=2", "--bearer", "${token}", "--raw"])
        .assert()
        .success()
        .stdout(contains("/x?a=1&b=2"))
        .stdout(contains("Bearer dev-123"));
    cmd(&home)
        .args(["send", &format!("{base}/k"), "--api-key", "api_key=s3", "--api-key-in", "query", "--raw"])
        .assert()
        .success()
        .stdout(contains("/k?api_key=s3"));
    cmd(&home)
        .args(["send", &format!("{base}/b"), "--basic", "ada:pw", "--raw"])
        .assert()
        .success()
        .stdout(contains("Basic YWRhOnB3"));
}

#[test]
fn send_post_script_assertions_set_the_exit_code() {
    let home = TempDir::new().unwrap();
    let base = echo_server();
    cmd(&home)
        .args(["send", &format!("{base}/ok"), "--raw", "--post-script", "assert.status(200); assert.bodyContains('target')"])
        .assert()
        .success()
        .stdout(contains("2/2 tests passed"));
    cmd(&home)
        .args(["send", &format!("{base}/ok"), "--raw", "--post-script", "assert.status(201)"])
        .assert()
        .failure()
        .stdout(contains("status is 201"))
        .stdout(contains("got 200"));
}

#[test]
fn requests_keep_auth_and_run_reports_failed_assertions() {
    let home = TempDir::new().unwrap();
    let base = echo_server();
    cmd(&home).args(["collections", "add", "Api"]).assert().success();
    cmd(&home)
        .args(["requests", "add", "Api", "--name", "Me", "--url", &format!("{base}/me"), "-H", "Accept: application/json", "-q", "v=2", "--bearer", "t0k"])
        .assert()
        .success();
    cmd(&home)
        .args(["requests", "show", "Api", "Me"])
        .assert()
        .success()
        .stdout(contains("/me?v=2"))
        .stdout(contains("Bearer token"))
        .stdout(contains("Accept"));
    cmd(&home).args(["requests", "send", "Api", "Me", "--raw"]).assert().success().stdout(contains("Bearer t0k"));
    cmd(&home).args(["run", "Api"]).assert().success().stdout(contains("1/1 requests succeeded"));

    // A failing check in the collection's file makes `run` fail.
    let cols = home.path().join("config").join("hitop").join("collections.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cols).unwrap()).unwrap();
    json[0]["requests"][0]["postRequestScript"] = "assert.header('x-missing')".into();
    std::fs::write(&cols, json.to_string()).unwrap();
    cmd(&home).args(["run", "Api"]).assert().failure().stdout(contains("0/1 requests succeeded")).stdout(contains("has header x-missing"));

    cmd(&home).args(["requests", "auth", "Api", "Me", "--none"]).assert().success();
    cmd(&home).args(["requests", "show", "Api", "Me"]).assert().success().stdout(contains("Auth").not());
}
