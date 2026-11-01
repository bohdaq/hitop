//! `${secret:vault|aws|azure|op:...}` references from the command line.
//!
//! 1Password and Azure run everywhere: a stub `op` and a stub `az` on PATH,
//! and a local mock for the Key Vault REST API. Vault and AWS Secrets Manager
//! run against real dev servers when they are given:
//!
//! - `HITOP_TEST_VAULT_ADDR` (+ `HITOP_TEST_VAULT_TOKEN`, default `dev-root`):
//!   Vault or OpenBao in dev mode, e.g. `docker run -p 8200:8200 -e
//!   VAULT_DEV_ROOT_TOKEN_ID=dev-root hashicorp/vault:1.20.4`;
//! - `HITOP_TEST_LOCALSTACK_URL`: LocalStack with Secrets Manager, e.g.
//!   `docker run -p 4566:4566 -e SERVICES=secretsmanager localstack/localstack:4.4`.
//!
//! Without them those tests are skipped, except on CI (`CI` set), where the
//! services run as containers and a missing one fails the test.
#![cfg(unix)]
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const OP_VALUE: &str = "op-T21-s3cr3t";
const AZ_VALUE: &str = "az-T21-s3cr3t";
const VAULT_VALUE: &str = "vault-T21-s3cr3t";
const VAULT_V1_VALUE: &str = "vault1-T21-s3cr3t";
const AWS_VALUE: &str = "aws-T21-s3cr3t";
const AWS_PLAIN: &str = "aws-plain-T21-s3cr3t";
const ALL: [&str; 6] = [OP_VALUE, AZ_VALUE, VAULT_VALUE, VAULT_V1_VALUE, AWS_VALUE, AWS_PLAIN];

struct Env {
    home: TempDir,
    bin: PathBuf,
    vars: Vec<(String, String)>,
}

impl Env {
    /// A home with no config, stub `op`, `az` and `aws` first on PATH.
    fn new() -> Env {
        let home = TempDir::new().unwrap();
        let bin = home.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let env = Env { home, bin, vars: vec![] };
        // `op read op://Dev/API/token --no-newline`.
        env.stub(
            "op",
            &format!(
                r#"[ "$1" = read ] && [ "$3" = --no-newline ] || {{ echo "unexpected: $*" >&2; exit 9; }}
case "$2" in
  op://Dev/API/token) printf '%s' '{OP_VALUE}' ;;
  *) echo "[ERROR] could not read secret '$2': isn't an item in the vault" >&2; exit 1 ;;
esac"#
            ),
        );
        env.stub(
            "az",
            r#"[ "$1 $2 $3 $4" = "account get-access-token --resource https://vault.azure.net" ] || { echo "unexpected: $*" >&2; exit 9; }
echo '{"accessToken": "az-cli-token", "expiresOn": "2099-01-01 00:00:00.000000", "tokenType": "Bearer"}'"#,
        );
        // Never the real AWS CLI.
        env.stub("aws", "echo 'Unable to locate credentials' >&2; exit 255");
        env
    }

    fn stub(&self, name: &str, body: &str) {
        let path = self.bin.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn set(&mut self, k: &str, v: &str) {
        self.vars.push((k.into(), v.into()));
    }

    fn hitop(&self, cwd: &Path) -> Command {
        let mut c = Command::cargo_bin("hitop").unwrap();
        let path = format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap_or_default());
        c.env("HOME", self.home.path()).env("XDG_CONFIG_HOME", self.home.path().join("config")).env("HITOP_KEYCHAIN", "off").env("PATH", path).current_dir(cwd);
        for k in [
            "VAULT_ADDR", "VAULT_TOKEN", "VAULT_NAMESPACE", "BAO_ADDR", "BAO_TOKEN", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "AWS_PROFILE", "AWS_REGION",
            "AWS_DEFAULT_REGION", "AWS_ENDPOINT_URL", "AWS_ENDPOINT_URL_SECRETS_MANAGER", "AZURE_CLIENT_ID", "AZURE_CLIENT_SECRET", "AZURE_TENANT_ID", "HITOP_SECRET_MANAGERS",
            "HITOP_OP_BIN", "HITOP_AZ_BIN", "HITOP_AWS_BIN",
        ] {
            c.env_remove(k);
        }
        for (k, v) in &self.vars {
            c.env(k, v);
        }
        c
    }

    /// Every file hitop keeps in its config dir (history, contexts, cookies).
    fn stored(&self) -> String {
        let mut all = String::new();
        let mut dirs = vec![self.home.path().join("config/hitop")];
        while let Some(d) = dirs.pop() {
            for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                if e.path().is_dir() {
                    dirs.push(e.path());
                } else {
                    all.push_str(&fs::read_to_string(e.path()).unwrap_or_default());
                }
            }
        }
        all
    }
}

fn output(o: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn no_value(what: &str, text: &str) {
    for v in ALL {
        assert!(!text.contains(v), "{what} holds a secret value ({v}):\n{text}");
    }
}

/// A Key Vault mock (token from the stub `az`) and an echo endpoint that
/// answers only with the 1Password and Azure values in place, and echoes them.
fn mocks(server: &MockServer) -> (httpmock::Mock<'_>, httpmock::Mock<'_>) {
    let vault = server.mock(|w, t| {
        w.method(GET).path("/kv-test/secrets/api-key").query_param("api-version", "7.4").header("authorization", "Bearer az-cli-token");
        t.status(200).json_body(serde_json::json!({"value": AZ_VALUE, "id": "https://kv-test.vault.azure.net/secrets/api-key/1"}));
    });
    let echo = server.mock(|w, t| {
        w.path("/echo").query_param("key", AZ_VALUE).header("authorization", format!("Bearer {OP_VALUE}"));
        t.status(201).header("x-echo", OP_VALUE).body(format!("{{\"key\": \"{AZ_VALUE}\", \"token\": \"{OP_VALUE}\"}}"));
    });
    (vault, echo)
}

/// A folder: the request takes the 1Password reference in a header; the
/// `prod` environment's `key` variable is an Azure reference.
fn folder(tmp: &TempDir, base: &str) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Api\nworkspace: ws-t21\n").unwrap();
    fs::write(
        dir.join("requests/echo.yaml"),
        "name: Echo\nurl: ${base}/echo?key=${key}\nheaders:\n- name: Authorization\n  value: Bearer ${secret:op://Dev/API/token}\nscripts:\n  post: |\n    assert.status(201);\n    assert.equal(1, 2, 'body is ' + pm.response.text());\n",
    )
    .unwrap();
    fs::write(dir.join("environments/prod.yaml"), format!("variables:\n  base: {base}\n  key: ${{secret:azure:kv-test/api-key}}\n")).unwrap();
    dir
}

fn azure_env(server: &MockServer) -> Env {
    let mut env = Env::new();
    env.set("HITOP_AZURE_VAULT_URL", &format!("{}/{{vault}}", server.base_url()));
    env
}

#[test]
fn run_resolves_op_and_azure_and_masks_every_report() {
    let server = MockServer::start();
    let (vault, echo) = mocks(&server);
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    let dir = folder(&tmp, &server.base_url());
    let out = env.hitop(tmp.path()).args(["run", "./api", "--env", "prod", "--reporter", "cli,junit,json,html", "--out", "out"]).assert().code(1).get_output().clone();
    echo.assert();
    // One Key Vault call for the whole run.
    vault.assert_calls(1);
    let log = output(&out);
    let reports: Vec<(String, String)> = ["junit.xml", "report.json", "report.html"].iter().map(|f| (f.to_string(), fs::read_to_string(tmp.path().join("out").join(f)).unwrap())).collect();
    no_value("log", &log);
    assert!(log.contains("body is {\"key\": \"••••\", \"token\": \"••••\"}"), "{log}");
    for (name, text) in &reports {
        no_value(name, text);
        assert!(text.contains("key=••••"), "{name}:\n{text}");
    }
    no_value("config dir", &env.stored());
    // The folder still holds the references, never a value.
    let files = fs::read_to_string(dir.join("environments/prod.yaml")).unwrap() + &fs::read_to_string(dir.join("requests/echo.yaml")).unwrap();
    assert!(files.contains("${secret:azure:kv-test/api-key}") && files.contains("${secret:op://Dev/API/token}"));
    no_value("folder", &files);
}

#[test]
fn send_and_requests_send_resolve_and_mask_output_and_history() {
    let server = MockServer::start();
    let (_, echo) = mocks(&server);
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    folder(&tmp, &server.base_url());
    let out = env
        .hitop(tmp.path())
        .args(["send", &format!("{}/echo?key=${{secret:azure:kv-test/api-key}}", server.base_url())])
        .args(["-H", "Authorization: Bearer ${secret:op:Dev/API/token}", "--show-headers", "--raw"])
        .assert()
        .success()
        .get_output()
        .clone();
    let log = output(&out);
    no_value("send", &log);
    assert!(log.contains("x-echo: ••••") && log.contains("\"token\": \"••••\""), "{log}");
    let out = env.hitop(tmp.path()).args(["requests", "send", "./api", "Echo", "--env", "prod", "--show-headers", "--raw"]).assert().get_output().clone();
    let log = output(&out);
    no_value("requests send", &log);
    assert!(log.contains("x-echo: ••••"), "{log}");
    echo.assert_calls(2);
    let kept = env.stored();
    no_value("history", &kept);
    assert!(kept.contains("key=••••"), "{kept}");
}

#[test]
fn errors_name_the_reference_and_the_switch_turns_it_off() {
    let server = MockServer::start();
    let (_, echo) = mocks(&server);
    server.mock(|w, t| {
        w.path("/kv-test/secrets/gone");
        t.status(404).json_body(serde_json::json!({"error": {"code": "SecretNotFound", "message": "A secret with (name/id) gone was not found in this key vault."}}));
    });
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    folder(&tmp, &server.base_url());
    let url = format!("{}/echo?a=${{secret:azure:kv-test/gone}}&b=${{secret:op:Dev/Nope/x}}", server.base_url());
    env.hitop(tmp.path())
        .args(["send", &url])
        .assert()
        .failure()
        .stderr(contains("could not resolve 2 secret references"))
        .stderr(contains("${secret:azure:kv-test/gone} (azure): Key Vault answered 404 Not Found SecretNotFound"))
        .stderr(contains("${secret:op:Dev/Nope/x} (op): `op read`"));
    env.hitop(tmp.path())
        .args(["run", "./api", "--env", "prod", "--no-secret-managers"])
        .assert()
        .code(2)
        .stderr(contains("turned off (--no-secret-managers)"))
        .stderr(contains("${secret:azure:kv-test/api-key}, ${secret:op://Dev/API/token}"));
    env.hitop(tmp.path()).args(["run", "./api", "--env", "prod"]).env("HITOP_SECRET_MANAGERS", "off").assert().code(2).stderr(contains("HITOP_SECRET_MANAGERS=off"));
    // A wrong reference is named, with the forms that work.
    env.hitop(tmp.path()).args(["send", &format!("{}/echo?x=${{secret:gcp:x}}", server.base_url())]).assert().failure().stderr(contains("unknown secret manager 'gcp'"));
    echo.assert_calls(0);
}

#[test]
fn secret_check_lists_and_tests_references_without_values() {
    let server = MockServer::start();
    mocks(&server);
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    let dir = folder(&tmp, &server.base_url());
    let out = env.hitop(tmp.path()).args(["secret", "check", "-c", "./api"]).assert().success().get_output().clone();
    let log = output(&out);
    no_value("check", &log);
    assert!(log.contains("✓ ${secret:azure:kv-test/api-key}  environment 'prod'"), "{log}");
    assert!(log.contains("✓ ${secret:op://Dev/API/token}  request 'Echo'"), "{log}");
    assert!(log.contains("2 secret references resolved"), "{log}");
    let listed = output(&env.hitop(tmp.path()).args(["secret", "check", "-c", "./api", "--list"]).env("PATH", "/nonexistent").assert().success().get_output().clone());
    assert!(listed.contains("${secret:op://Dev/API/token}") && !listed.contains('✓'), "{listed}");
    // One that doesn't resolve: exit 1, the reason, still no value.
    fs::write(dir.join("globals.yaml"), "variables:\n  broken: ${secret:op:Dev/Missing/field}\n").unwrap();
    let out = env.hitop(tmp.path()).args(["secret", "check", "-c", "./api"]).assert().code(1).get_output().clone();
    let log = output(&out);
    no_value("check", &log);
    assert!(log.contains("✗ ${secret:op:Dev/Missing/field}  globals") && log.contains("isn't an item in the vault"), "{log}");
    assert!(log.contains("1 of 3 secret references did not resolve"), "{log}");
}

#[test]
fn snippets_keep_references_as_placeholders() {
    let server = MockServer::start();
    let (vault, _) = mocks(&server);
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    folder(&tmp, &server.base_url());
    let out = env.hitop(tmp.path()).args(["snippet", "./api", "Echo", "--env", "prod", "--lang", "curl"]).assert().success().get_output().clone();
    let code = String::from_utf8_lossy(&out.stdout);
    assert!(code.contains("{{secret:azure:kv-test/api-key}}") && code.contains("{{secret:op://Dev/API/token}}"), "{code}");
    vault.assert_calls(0);
    let out = env.hitop(tmp.path()).args(["snippet", "./api", "Echo", "--env", "prod", "--lang", "curl", "--show-secrets"]).assert().success().get_output().clone();
    let code = String::from_utf8_lossy(&out.stdout);
    assert!(code.contains(AZ_VALUE) && code.contains(OP_VALUE), "{code}");
}

#[test]
fn mcp_runs_resolve_references_and_mask_them() {
    let server = MockServer::start();
    let (_, echo) = mocks(&server);
    let env = azure_env(&server);
    let tmp = TempDir::new().unwrap();
    let dir = folder(&tmp, &server.base_url());
    let lines = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_request","arguments":{"request":"Echo","environment":"prod"}}}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"run_collection","arguments":{"environment":"prod"}}}"#,
    ];
    let out = env.hitop(tmp.path()).args(["mcp", dir.to_str().unwrap()]).write_stdin(lines.join("\n") + "\n").assert().success().get_output().clone();
    let log = output(&out);
    no_value("mcp", &log);
    assert!(log.contains("key=••••"), "{log}");
    echo.assert_calls(2);
}

// ── Real dev servers ────────────────────────────────────────────────────────

/// The dev server's address, or None to skip (fails on CI).
fn service(var: &str) -> Option<String> {
    match std::env::var(var).ok().filter(|v| !v.is_empty()) {
        Some(v) => Some(v.trim_end_matches('/').to_string()),
        None if std::env::var_os("CI").is_some() => panic!("{var} is not set: CI runs this test against a service container"),
        None => {
            eprintln!("skipped: set {var} to run this test against a dev server");
            None
        }
    }
}

#[test]
fn vault_dev_server_kv2_and_kv1() {
    let Some(addr) = service("HITOP_TEST_VAULT_ADDR") else { return };
    let token = std::env::var("HITOP_TEST_VAULT_TOKEN").unwrap_or_else(|_| "dev-root".into());
    let http = reqwest::blocking::Client::new();
    let put = |path: &str, body: serde_json::Value| {
        let r = http.post(format!("{addr}/v1/{path}")).header("X-Vault-Token", &token).json(&body).send().unwrap();
        (r.status().as_u16(), r.text().unwrap_or_default())
    };
    // Dev mode mounts KV v2 at secret/; add a KV v1 mount next to it.
    let (status, text) = put("secret/data/hitop-t21", serde_json::json!({"data": {"token": VAULT_VALUE, "user": "svc"}}));
    assert_eq!(status, 200, "{text}");
    let (status, text) = put("sys/mounts/kv1-hitop-t21", serde_json::json!({"type": "kv", "options": {"version": "1"}}));
    assert!(status == 204 || text.contains("already in use"), "{status} {text}");
    let (status, text) = put("kv1-hitop-t21/app", serde_json::json!({"password": VAULT_V1_VALUE}));
    assert_eq!(status, 204, "{text}");

    let server = MockServer::start();
    let echo = server.mock(|w, t| {
        w.path("/echo").header("x-token", VAULT_VALUE).header("x-password", VAULT_V1_VALUE).query_param("user", "svc");
        t.status(200).body(format!("{VAULT_VALUE} {VAULT_V1_VALUE}"));
    });
    let mut env = Env::new();
    env.set("VAULT_ADDR", &addr);
    // The token from ~/.vault-token, as `vault login` leaves it.
    fs::write(env.home.path().join(".vault-token"), format!("{token}\n")).unwrap();
    let tmp = TempDir::new().unwrap();
    let out = env
        .hitop(tmp.path())
        .args(["send", &format!("{}/echo?user=${{secret:vault:secret/hitop-t21#user}}", server.base_url()), "--raw"])
        .args(["-H", "X-Token: ${secret:vault:secret/data/hitop-t21#token}", "-H", "X-Password: ${secret:vault:kv1-hitop-t21/app}"])
        .assert()
        .success()
        .get_output()
        .clone();
    echo.assert();
    let log = output(&out);
    no_value("send", &log);
    assert!(log.contains("•••• ••••"), "{log}");
    no_value("history", &env.stored());
    // A wrong token: the reference and Vault's reason, nothing else.
    env.set("VAULT_TOKEN", "not-a-token");
    env.hitop(tmp.path())
        .args(["send", &format!("{}/echo?x=${{secret:vault:secret/hitop-t21#token}}", server.base_url())])
        .assert()
        .failure()
        .stderr(contains("${secret:vault:secret/hitop-t21#token} (vault): permission denied"));
}

#[test]
fn localstack_secrets_manager() {
    let Some(endpoint) = service("HITOP_TEST_LOCALSTACK_URL") else { return };
    let http = reqwest::blocking::Client::new();
    // LocalStack doesn't check signatures; the region comes from the scope.
    let call = |target: &str, body: serde_json::Value| -> serde_json::Value {
        let r = http
            .post(&endpoint)
            .header("X-Amz-Target", format!("secretsmanager.{target}"))
            .header("Content-Type", "application/x-amz-json-1.1")
            .header("Authorization", "AWS4-HMAC-SHA256 Credential=test/20261101/us-east-1/secretsmanager/aws4_request, SignedHeaders=host, Signature=0")
            .body(body.to_string())
            .send()
            .unwrap();
        r.json().unwrap_or(serde_json::Value::Null)
    };
    let suffix = std::process::id();
    let json_name = format!("hitop-t21/db-{suffix}");
    let created = call("CreateSecret", serde_json::json!({"Name": json_name, "ClientRequestToken": format!("00000000-0000-4000-8000-{suffix:012}"), "SecretString": serde_json::json!({"password": AWS_VALUE, "port": 5432}).to_string()}));
    let arn = created["ARN"].as_str().unwrap_or_else(|| panic!("CreateSecret: {created}")).to_string();
    let plain_name = format!("hitop-t21/plain-{suffix}");
    call("CreateSecret", serde_json::json!({"Name": plain_name, "ClientRequestToken": format!("00000000-0000-4000-9000-{suffix:012}"), "SecretString": AWS_PLAIN}));

    let server = MockServer::start();
    let echo = server.mock(|w, t| {
        w.path("/echo").header("x-password", AWS_VALUE).header("x-plain", AWS_PLAIN).query_param("port", "5432").query_param("again", AWS_VALUE);
        t.status(200).body(format!("{AWS_VALUE} {AWS_PLAIN}"));
    });
    let mut env = Env::new();
    for (k, v) in [("AWS_ENDPOINT_URL", endpoint.as_str()), ("AWS_REGION", "us-east-1"), ("AWS_ACCESS_KEY_ID", "test"), ("AWS_SECRET_ACCESS_KEY", "test")] {
        env.set(k, v);
    }
    let tmp = TempDir::new().unwrap();
    let url = format!("{}/echo?port=${{secret:aws:{json_name}#port}}&again=${{secret:aws:{arn}#password}}", server.base_url());
    let out = env
        .hitop(tmp.path())
        .args(["send", &url, "--raw"])
        .args(["-H", &format!("X-Password: ${{secret:aws:{json_name}#password}}"), "-H", &format!("X-Plain: ${{secret:aws:{plain_name}}}")])
        .assert()
        .success()
        .get_output()
        .clone();
    echo.assert();
    let log = output(&out);
    no_value("send", &log);
    assert!(log.contains("•••• ••••"), "{log}");
    no_value("history", &env.stored());
    env.hitop(tmp.path())
        .args(["send", &format!("{}/echo?x=${{secret:aws:hitop-t21/missing-{suffix}}}", server.base_url())])
        .assert()
        .failure()
        .stderr(contains(format!("${{secret:aws:hitop-t21/missing-{suffix}}} (aws): Secrets Manager answered 400 Bad Request ResourceNotFoundException")));
}

/// The desktop's Settings file (secret-managers.json in hitop's settings
/// folder) is read by the CLI too: it fills in what the environment leaves
/// out, and the environment wins.
#[test]
fn the_settings_file_fills_in_and_the_environment_wins() {
    let mut env = Env::new();
    let server = MockServer::start();
    server.mock(|w, t| {
        w.method(GET).path("/v1/sys/internal/ui/mounts/secret/app").header("X-Vault-Token", "dev-token");
        t.status(200).json_body(serde_json::json!({"data": {"path": "secret/", "type": "kv", "options": {"version": "2"}}}));
    });
    server.mock(|w, t| {
        w.method(GET).path("/v1/secret/data/app").header("X-Vault-Token", "dev-token");
        t.status(200).json_body(serde_json::json!({"data": {"data": {"token": VAULT_VALUE}, "metadata": {}}}));
    });
    // `op` only where the settings say, not on PATH.
    let tools = env.home.path().join("tools");
    fs::create_dir(&tools).unwrap();
    fs::rename(env.bin.join("op"), tools.join("op-cli")).unwrap();
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Api\n").unwrap();
    fs::write(dir.join("environments/dev.yaml"), "variables:\n  a: ${secret:op:Dev/API/token}\n  b: ${secret:vault:secret/app#token}\n").unwrap();
    env.set("VAULT_TOKEN", "dev-token");
    let check = |env: &Env| env.hitop(tmp.path()).args(["secret", "check", "-c", dir.to_str().unwrap()]).output().unwrap();

    let o = check(&env);
    let text = output(&o);
    assert!(!o.status.success() && text.contains("not found on PATH") && text.contains("VAULT_ADDR is not set"), "{text}");

    let settings = env.home.path().join("config/hitop/secret-managers.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, serde_json::json!({"opPath": tools.join("op-cli"), "vaultAddr": server.base_url()}).to_string()).unwrap();
    let o = check(&env);
    let text = output(&o);
    assert!(o.status.success() && text.matches("✓ ${secret:").count() == 2 && text.contains("secret-managers.json"), "{text}");
    no_value("secret check", &text);

    // The environment wins over the file.
    env.set("HITOP_OP_BIN", "/nonexistent-op");
    env.set("BAO_ADDR", "http://127.0.0.1:9");
    let o = check(&env);
    let text = output(&o);
    assert!(!o.status.success() && text.contains("/nonexistent-op") && text.matches("✗ ${secret:").count() == 2, "{text}");

    // A broken file is an error naming it.
    fs::write(&settings, "{nope").unwrap();
    let text = output(&check(&env));
    assert!(text.contains("secret-managers.json") && text.contains("not valid"), "{text}");
}
