//! Plugins (docs/PLUGINS_THREAT_MODEL.md): `hitop plugin install|ls|rm|call`,
//! `${plugin.name(args)}` in a run, the child process's limits, the hash
//! check, and masking of an answer computed from a secret.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

const SECRET: &str = "user:s3cr3t-Plugin";
/// base64 of SECRET.
const SECRET_B64: &str = "dXNlcjpzM2NyM3QtUGx1Z2lu";

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .current_dir(cwd);
    c
}

const B64: &str = r#"/* hitop-plugin {"name": "b64", "version": "1.0.0", "hooks": ["value"], "description": "base64"} */
function value(text) {
  const t = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"; let o = "", i = 0;
  for (; i + 2 < text.length; i += 3) { const n = (text.charCodeAt(i) << 16) | (text.charCodeAt(i + 1) << 8) | text.charCodeAt(i + 2); o += t[n >> 18 & 63] + t[n >> 12 & 63] + t[n >> 6 & 63] + t[n & 63]; }
  const r = text.length - i;
  if (r) { const n = (text.charCodeAt(i) << 16) | (r > 1 ? text.charCodeAt(i + 1) << 8 : 0); o += t[n >> 18 & 63] + t[n >> 12 & 63] + (r > 1 ? t[n >> 6 & 63] : "=") + "="; }
  return o;
}
"#;

fn plugin(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(format!("{name}.js"));
    fs::write(&p, format!("/* hitop-plugin {{\"name\": \"{name}\", \"version\": \"1\", \"hooks\": [\"value\"]}} */\n{body}\n")).unwrap();
    p
}

fn install(home: &TempDir, file: &Path) {
    hitop(home, home.path()).args(["plugin", "install", file.to_str().unwrap()]).assert().success();
}

#[test]
fn install_list_call_and_remove() {
    let home = TempDir::new().unwrap();
    let file = home.path().join("b64.js");
    fs::write(&file, B64).unwrap();
    hitop(&home, home.path()).args(["plugin", "ls"]).assert().success().stdout(contains("No plugins installed"));
    hitop(&home, home.path()).args(["plugin", "install", file.to_str().unwrap()]).assert().success().stdout(contains("Installed plugin b64 1.0.0"));
    hitop(&home, home.path()).args(["plugin", "call", "b64", SECRET]).assert().success().stdout(format!("{SECRET_B64}\n"));
    // A second install of the same name asks for --force and shows both versions.
    fs::write(&file, B64.replace("\"1.0.0\"", "\"2.0.0\"")).unwrap();
    hitop(&home, home.path())
        .args(["plugin", "install", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(contains("installed already (version 1.0.0").and(contains("this file is version 2.0.0")).and(contains("--force")));
    hitop(&home, home.path()).args(["plugin", "install", file.to_str().unwrap(), "--force"]).assert().success();
    let ls = hitop(&home, home.path()).args(["plugin", "ls", "--json"]).assert().success().get_output().stdout.clone();
    let ls: serde_json::Value = serde_json::from_slice(&ls).unwrap();
    assert_eq!(ls[0]["name"], "b64");
    assert_eq!(ls[0]["version"], "2.0.0");
    assert_eq!(ls[0]["sha256"].as_str().unwrap().len(), 64);
    // A changed file is refused until it is installed again.
    let installed = home.path().join("config/hitop/plugins/b64.js");
    fs::write(&installed, B64.replace("return o;", "return o + 'x';")).unwrap();
    hitop(&home, home.path()).args(["plugin", "call", "b64", "a"]).assert().failure().stderr(contains("changed since it was installed"));
    hitop(&home, home.path()).args(["plugin", "rm", "b64"]).assert().success();
    assert!(!installed.exists());
    hitop(&home, home.path()).args(["plugin", "call", "b64", "a"]).assert().failure().stderr(contains("plugin 'b64', which isn't installed"));
    // Bad manifests are refused with the reason.
    let bad = home.path().join("bad.js");
    fs::write(&bad, "function value() {}").unwrap();
    hitop(&home, home.path()).args(["plugin", "install", bad.to_str().unwrap()]).assert().failure().stderr(contains("starts with its manifest"));
}

#[test]
fn a_plugin_finds_no_way_out_and_is_stopped() {
    let home = TempDir::new().unwrap();
    let d = home.path();
    install(&home, &plugin(d, "probe", "function value() { return [typeof process, typeof require, typeof fetch, typeof console, typeof localStorage, typeof document, typeof XMLHttpRequest, typeof Deno, typeof os, typeof std, Object.getOwnPropertyNames(globalThis).filter(n => !/^[A-Z]/.test(n) && !['globalThis', 'value', 'escape', 'unescape', 'eval', 'isFinite', 'isNaN', 'parseFloat', 'parseInt', 'decodeURI', 'decodeURIComponent', 'encodeURI', 'encodeURIComponent', 'undefined', 'NaN', 'Infinity'].includes(n)).join('+')].join(); }"));
    hitop(&home, d)
        .args(["plugin", "call", "probe"])
        .assert()
        .success()
        .stdout("undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,undefined,\n");
    install(&home, &plugin(d, "spin", "function value() { for (;;) {} }"));
    install(&home, &plugin(d, "deep", "function value() { return value(); }"));
    install(&home, &plugin(d, "huge", "function value() { return 'a'.repeat(1e9); }"));
    install(&home, &plugin(d, "regex", "function value() { return String(/^(a+)+$/.test('a'.repeat(40) + 'b')); }"));
    install(&home, &plugin(d, "big", "function value() { return 'a'.repeat(2 * 1024 * 1024); }"));
    install(&home, &plugin(d, "num", "function value() { return 42; }"));
    for (name, says) in [
        ("spin", "plugin 'spin'"),
        ("deep", "plugin 'deep' failed"),
        ("huge", "plugin 'huge' stopped: it used too much memory"),
        ("regex", "plugin 'regex' was stopped: it ran longer than 5 s"),
        ("big", "more than 1048576 bytes"),
        ("num", "answered something other than text"),
    ] {
        let started = Instant::now();
        hitop(&home, d).args(["plugin", "call", name]).assert().failure().stderr(contains(says));
        assert!(started.elapsed() < Duration::from_secs(15), "{name} took {:?}", started.elapsed());
    }
}

/// The child gets an empty environment: read from /proc while it runs.
#[cfg(target_os = "linux")]
#[test]
fn the_child_sees_no_environment_variables() {
    let home = TempDir::new().unwrap();
    install(&home, &plugin(home.path(), "slow", "function value() { return String(/^(a+)+$/.test('a'.repeat(40) + 'b')); }"));
    let exe = assert_cmd::cargo::cargo_bin("hitop");
    let mut parent = std::process::Command::new(&exe)
        .args(["plugin", "call", "slow"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_SECRET_CI_TOKEN", "do-not-pass-me")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let parent_pid = parent.id().to_string();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut environ = None;
    while environ.is_none() && Instant::now() < deadline {
        for e in fs::read_dir("/proc").unwrap().flatten() {
            let stat = fs::read_to_string(e.path().join("stat")).unwrap_or_default();
            // pid (comm) state ppid ...
            let ppid = stat.rsplit_once(") ").and_then(|(_, rest)| rest.split(' ').nth(1));
            let cmdline = fs::read(e.path().join("cmdline")).unwrap_or_default();
            if ppid == Some(parent_pid.as_str()) && cmdline.ends_with(b"plugin-run\0") {
                environ = fs::read(e.path().join("environ")).ok();
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = parent.kill();
    let _ = parent.wait();
    assert_eq!(environ.expect("the plugin-run child was not found"), b"", "the child has environment variables");
}

#[test]
fn a_run_sends_the_answer_and_masks_it_when_it_came_from_a_secret() {
    let server = MockServer::start();
    let seen = server.mock(|when, then| {
        when.path("/p").query_param("sig", SECRET_B64).header("authorization", format!("Basic {SECRET_B64}")).header("x-plain", "aGk=");
        then.status(200).body(format!("echo {SECRET_B64}"));
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let file = tmp.path().join("b64.js");
    fs::write(&file, B64).unwrap();
    install(&home, &file);
    let api = tmp.path().join("api");
    fs::create_dir_all(api.join("requests")).unwrap();
    fs::create_dir_all(api.join("environments")).unwrap();
    fs::write(api.join("hitop.yaml"), "format: 1\nname: P\n").unwrap();
    fs::write(api.join("environments/ci.yaml"), format!("variables:\n  base: {}\n  cred:\n    secret: true\n", server.base_url())).unwrap();
    fs::write(
        api.join("requests/p.yaml"),
        "name: P\nmethod: GET\nurl: ${base}/p?sig=${plugin.b64(cred)}\nheaders:\n- name: Authorization\n  value: Basic ${plugin.b64(cred)}\n- name: X-Plain\n  value: ${plugin.b64(\"hi\")}\nscripts:\n  post: assert.status(200);\n",
    )
    .unwrap();
    let out = hitop(&home, tmp.path())
        .args(["run", "./api", "--env", "ci", "--reporter", "cli,json,junit,html", "--out", "out"])
        .env("HITOP_SECRET_CI_CRED", SECRET)
        .assert()
        .code(0)
        .get_output()
        .clone();
    seen.assert();
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    for (what, text) in [
        ("log", log),
        ("json", fs::read_to_string(tmp.path().join("out/report.json")).unwrap()),
        ("junit", fs::read_to_string(tmp.path().join("out/junit.xml")).unwrap()),
        ("html", fs::read_to_string(tmp.path().join("out/report.html")).unwrap()),
    ] {
        assert!(!text.contains(SECRET_B64) && !text.contains(SECRET), "{what} holds the secret or its base64:\n{text}");
        assert!(what == "log" || text.contains("sig=••••"), "{what} has no mask:\n{text}");
    }
    // Nor does anything hitop keeps (history).
    for e in fs::read_dir(home.path().join("config/hitop")).unwrap().flatten().filter(|e| e.path().is_file()) {
        let text = fs::read_to_string(e.path()).unwrap_or_default();
        assert!(!text.contains(SECRET_B64), "{} holds the base64 of the secret", e.path().display());
    }
    // Not installed: nothing is sent and the plugin is named.
    hitop(&home, tmp.path()).args(["plugin", "rm", "b64"]).assert().success();
    let out = hitop(&home, tmp.path()).args(["run", "./api", "--env", "ci"]).env("HITOP_SECRET_CI_CRED", SECRET).assert().code(1).get_output().clone();
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(log.contains("plugin 'b64', which isn't installed"), "{log}");
    seen.assert_calls(1);
}

