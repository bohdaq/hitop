//! Secret variables from the command line: `--secret`, the
//! `HITOP_SECRET_<ENV>_<NAME>` env var, `hitop secret`, and masking. The
//! keychain is turned off (`HITOP_KEYCHAIN=off`), as on a headless CI
//! runner, so these tests never touch the real one; the keychain path is
//! covered with a mock store in hitop-core. HTTP goes to a local mock server.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

const SECRET: &str = "s3cr3t-Value_42";

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .env_remove("HITOP_SECRET_PROD_TOKEN")
        .current_dir(cwd);
    c
}

fn config(home: &TempDir) -> PathBuf {
    home.path().join("config/hitop")
}

/// Every file hitop keeps in its config dir, concatenated.
fn stored(home: &TempDir) -> String {
    let mut all = String::new();
    if let Ok(entries) = fs::read_dir(config(home)) {
        for e in entries {
            all.push_str(&fs::read_to_string(e.unwrap().path()).unwrap_or_default());
        }
    }
    all
}

fn echo_mock(server: &MockServer) -> httpmock::Mock<'_> {
    server.mock(|when, then| {
        when.method(GET).path("/echo").query_param("key", SECRET).header("authorization", format!("Bearer {SECRET}"));
        then.status(200).header("x-echo", SECRET).body(format!("{{\"token\": \"{SECRET}\"}}"));
    })
}

#[test]
fn send_masks_a_secret_in_output_and_history() {
    let server = MockServer::start();
    let echo = echo_mock(&server);
    let home = TempDir::new().unwrap();
    let out = hitop(&home, home.path())
        .args(["send", &format!("{}/echo?key=${{token}}", server.base_url())])
        .args(["-H", "Authorization: Bearer ${token}", "--show-headers", "--raw"])
        .args(["--secret", &format!("token={SECRET}")])
        .args(["--post-script", &format!("assert.bodyContains('{SECRET}'); assert.equal(1, 2, 'token is {SECRET}')")])
        .assert()
        .failure()
        .get_output()
        .clone();
    echo.assert();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("x-echo: ••••"), "{stdout}");
    assert!(stdout.contains("{\"token\": \"••••\"}"), "{stdout}");
    // The script saw the real body: bodyContains passed.
    assert!(stdout.contains("✓ body contains \"••••\""), "{stdout}");
    assert!(stdout.contains("token is ••••"), "{stdout}");
    assert!(!stdout.contains(SECRET), "{stdout}");
    let kept = stored(&home);
    assert!(kept.contains("key=••••"), "{kept}");
    assert!(!kept.contains(SECRET), "history keeps no secret: {kept}");
}

/// A folder whose `prod` environment has a secret `token`.
fn folder(tmp: &TempDir, base: &str) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Api\nworkspace: ws-test\n").unwrap();
    fs::write(
        dir.join("requests/echo.yaml"),
        "name: Echo\nurl: ${base}/echo?key=${token}\nheaders:\n- name: Authorization\n  value: Bearer ${token}\nscripts:\n  post: assert.status(201);\n",
    )
    .unwrap();
    fs::write(dir.join("environments/prod.yaml"), format!("variables:\n  base: {base}\n  token:\n    secret: true\n")).unwrap();
    dir
}

#[test]
fn run_takes_a_secret_from_the_env_var_and_masks_the_log() {
    let server = MockServer::start();
    let echo = echo_mock(&server);
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    folder(&tmp, &server.base_url());
    let out = hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "prod"])
        .env("HITOP_SECRET_PROD_TOKEN", SECRET)
        .assert()
        // assert.status(201) fails on the 200: the run exits 1, still masked.
        .failure()
        .stdout(contains("Echo").and(contains("status is 201")))
        .get_output()
        .clone();
    echo.assert();
    let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!all.contains(SECRET), "{all}");
    // Never asked the keychain, so no note about it.
    assert!(!all.contains("keychain"), "{all}");
    assert!(!stored(&home).contains(SECRET));
    // The folder files are untouched and hold no value.
    assert!(!fs::read_to_string(tmp.path().join("api/environments/prod.yaml")).unwrap().contains(SECRET));
}

#[test]
fn run_takes_a_secret_from_the_flag() {
    let server = MockServer::start();
    let echo = echo_mock(&server);
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    folder(&tmp, &server.base_url());
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "prod", "--secret", &format!("token={SECRET}")])
        .assert()
        .failure()
        .stdout(contains("status is 201").and(contains(SECRET).not()));
    echo.assert();
}

#[test]
fn a_missing_secret_without_a_terminal_says_how_to_give_it() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    folder(&tmp, "http://127.0.0.1:9");
    hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "prod"])
        .assert()
        .failure()
        .stderr(
            contains("no value for secret token")
                .and(contains("HITOP_SECRET_PROD_TOKEN"))
                .and(contains("--secret token="))
                .and(contains("no OS keychain available"))
                .and(contains("panicked").not()),
        );
}

#[test]
fn secret_commands_without_a_keychain_point_to_the_env_var() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    folder(&tmp, "http://127.0.0.1:9");
    hitop(&home, tmp.path())
        .args(["secret", "set", "token", "x", "--env", "prod", "-c", "./api"])
        .assert()
        .failure()
        .stderr(contains("no OS keychain available").and(contains("HITOP_SECRET_PROD_TOKEN")));
    hitop(&home, tmp.path())
        .args(["secret", "ls", "--env", "prod", "-c", "./api"])
        .assert()
        .success()
        .stdout(contains("token").and(contains("unknown (no keychain)")));
    hitop(&home, tmp.path())
        .args(["secret", "ls", "--env", "prod", "-c", "./api"])
        .env("HITOP_SECRET_PROD_TOKEN", SECRET)
        .assert()
        .success()
        .stdout(contains("env var HITOP_SECRET_PROD_TOKEN").and(contains(SECRET).not()));
}

#[test]
fn environments_never_show_or_export_a_secret_value() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(config(&home)).unwrap();
    // A value under a secret name (a hand edit) must not leak either.
    fs::write(
        config(&home).join("environments.json"),
        format!(r#"{{"active":null,"environments":[{{"id":"p","name":"prod","variables":{{"base":"https://x","token":"{SECRET}"}},"secrets":["token"]}}]}}"#),
    )
    .unwrap();
    hitop(&home, home.path())
        .args(["env", "show", "prod"])
        .assert()
        .success()
        .stdout(contains("base").and(contains("token = ••••")).and(contains(SECRET).not()));
    hitop(&home, home.path()).args(["env", "set", "prod", "token", "x"]).assert().failure().stderr(contains("hitop secret set token --env prod"));
    let file = home.path().join("envs.json");
    hitop(&home, home.path()).args(["env", "export", file.to_str().unwrap()]).assert().success();
    let exported = fs::read_to_string(&file).unwrap();
    assert!(!exported.contains(SECRET), "{exported}");
    assert!(exported.contains("\"secrets\""), "{exported}");
    // Importing it back keeps the name a secret, with no value.
    hitop(&home, home.path()).args(["env", "import", file.to_str().unwrap()]).assert().success();
    let envs = fs::read_to_string(config(&home).join("environments.json")).unwrap();
    assert_eq!(envs.matches(SECRET).count(), 1, "only the original hand-edited value: {envs}");
}

#[test]
fn convert_writes_no_secret_value() {
    let home = TempDir::new().unwrap();
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("api.json"), r#"[{"id":1,"name":"Api","requests":[]}]"#).unwrap();
    fs::write(
        tmp.path().join("envs.json"),
        format!(r#"[{{"id":"prod","name":"prod","variables":{{"token":"{SECRET}"}},"secrets":["token"]}}]"#),
    )
    .unwrap();
    hitop(&home, tmp.path()).args(["convert", "api.json", "out", "--environments", "envs.json"]).assert().success();
    let env = fs::read_to_string(tmp.path().join("out/environments/prod.yaml")).unwrap();
    assert_eq!(env, "variables:\n  token:\n    secret: true\n");
}
