use assert_cmd::Command;
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
        .stdout(contains("0.1.0"));
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
