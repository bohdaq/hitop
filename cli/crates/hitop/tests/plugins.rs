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


/// Spec 5c: in one request the same call with the same argument values is
/// made once (a nonce in the URL and a header is one nonce); the next
/// request makes it again.
#[test]
fn the_same_call_in_one_request_is_one_answer() {
    let server = MockServer::start();
    let seen = server.mock(|when, then| {
        when.path("/n");
        then.status(200).body("ok");
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    install(&home, &plugin(tmp.path(), "nonce", "function value() { return String(Math.random()).slice(2); }"));
    let api = tmp.path().join("api");
    fs::create_dir_all(api.join("requests")).unwrap();
    fs::write(api.join("hitop.yaml"), format!("format: 1\nname: N\nvariables:\n  base: {}\n", server.base_url())).unwrap();
    let post = "post: |\n    const n = pm.request.url.toString().split('n=')[1];\n    pm.test('one nonce', () => pm.expect(pm.request.headers.get('X-Nonce')).to.eql(n));\n    pm.test('same as before: ' + pm.environment.get('last'), () => pm.expect(n).to.not.eql(pm.environment.get('last')));\n    pm.environment.set('last', n);\n";
    for name in ["a", "b"] {
        fs::write(
            api.join(format!("requests/{name}.yaml")),
            format!("name: {name}\nmethod: GET\nurl: ${{base}}/n?n=${{plugin.nonce()}}\nheaders:\n- name: X-Nonce\n  value: ${{plugin.nonce()}}\nscripts:\n  {post}"),
        )
        .unwrap();
    }
    let out = hitop(&home, tmp.path()).args(["run", "./api"]).assert().get_output().clone();
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && log.matches("one nonce").count() == 2, "{log}");
    seen.assert_calls(2);
}

fn hooked(dir: &Path, name: &str, hook: &str, body: &str) -> PathBuf {
    let p = dir.join(format!("{name}.js"));
    fs::write(&p, format!("/* hitop-plugin {{\"name\": \"{name}\", \"version\": \"1\", \"hooks\": [\"{hook}\"]}} */\n{body}\n")).unwrap();
    p
}

/// A folder collection with one request that sends the secret `cred`.
fn secret_api(tmp: &Path, base: &str) -> PathBuf {
    let api = tmp.join("api");
    fs::create_dir_all(api.join("requests")).unwrap();
    fs::create_dir_all(api.join("environments")).unwrap();
    fs::write(api.join("hitop.yaml"), "format: 1\nname: P\n").unwrap();
    fs::write(api.join("environments/ci.yaml"), format!("variables:\n  base: {base}\n  cred:\n    secret: true\n")).unwrap();
    fs::write(api.join("requests/p.yaml"), "name: P\nmethod: POST\nurl: ${base}/p\nheaders:\n- name: Authorization\n  value: Bearer ${cred}\nbody:\n  type: text\n  content: hello\n").unwrap();
    api
}

/// Decisions 3, 8 and 9: a viewer gets the response with secrets masked
/// (headers too), its table is printed as text, and a wrong table, or a body
/// that isn't text, is refused.
#[test]
fn a_viewer_gets_a_masked_text_response_and_its_table_is_text() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.path("/p");
        then.status(200).header("x-token", SECRET).body(format!("id,who\n1,{SECRET}\n2,ann\n"));
    });
    server.mock(|when, then| {
        when.path("/bin");
        then.status(200).body(vec![0xffu8, 0xfe, 0x00, 0x81]);
    });
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    install(
        &home,
        &hooked(
            tmp.path(),
            "csv",
            "viewer",
            "function viewer(r) { const l = r.body.trim().split('\\n'); return {columns: l[0].split(','), rows: l.slice(1).map(x => x.split(',')).concat([['<script>x</script>', null], [r.headers.find(h => h.name === 'x-token').value, r.status]])}; }",
        ),
    );
    secret_api(tmp.path(), &server.base_url());
    let send = |extra: &[&str]| {
        let mut c = hitop(&home, tmp.path());
        c.args(["requests", "send", "./api", "P", "--env", "ci", "--view", "csv"]).args(extra).env("HITOP_SECRET_CI_CRED", SECRET);
        c.assert().get_output().clone()
    };
    let out = send(&[]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(!text.contains(SECRET), "the viewer got or printed the secret:\n{text}");
    assert!(text.contains("<script>x</script>") && text.contains("ann") && text.matches("••••").count() == 2 && text.contains("200"), "{text}");
    let raw = String::from_utf8_lossy(&send(&["--raw"]).stdout).to_string();
    assert!(raw.contains("id\twho\n1\t••••\n2\tann\n"), "{raw}");

    // The same request through `hitop send` (the secret given with --secret).
    let out = hitop(&home, tmp.path())
        .args(["send", &format!("{}/p", server.base_url()), "-H", "Authorization: Bearer ${cred}", "--secret", &format!("cred={SECRET}"), "--view", "csv"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!text.contains(SECRET) && text.contains("ann"), "{text}");

    // The answer is masked too: a viewer that puts the secret together from
    // pieces (each too short to be masked going in) prints ••••. Nor does
    // its text drive the terminal.
    server.mock(|when, then| {
        when.path("/pieces");
        then.status(200).body(format!("{}\n{}\n", &SECRET[..9], &SECRET[9..]));
    });
    install(&home, &hooked(tmp.path(), "join", "viewer", "function viewer(r) { return 'got ' + r.body.split('\\n').join('') + ' \\u001b[31mred'; }"));
    let out = hitop(&home, tmp.path())
        .args(["send", &format!("{}/pieces", server.base_url()), "--secret", &format!("cred={SECRET}"), "-H", "X-C: ${cred}", "--view", "join"])
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("got ••••") && !text.contains(SECRET) && !text.contains('\u{1b}') && text.contains("\u{fffd}[31mred"), "{text}");

    // Too many columns, a row too long, an answer that isn't text or a table.
    for (name, body, says) in [
        ("wide", "function viewer() { return {columns: Array.from({length: 101}, (_, i) => 'c' + i), rows: []}; }", "101 columns"),
        ("long", "function viewer() { return {columns: ['a'], rows: [['1', '2']]}; }", "row 1 has 2 cells for 1 columns"),
        ("num", "function viewer() { return 7; }", "other than text or a table"),
    ] {
        install(&home, &hooked(tmp.path(), name, "viewer", body));
        let out = hitop(&home, tmp.path()).args(["requests", "send", "./api", "P", "--env", "ci", "--view", name]).env("HITOP_SECRET_CI_CRED", SECRET).assert().failure().get_output().clone();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(says), "{name}: {err}");
    }
    // A body that isn't text isn't given to a viewer.
    let out = hitop(&home, tmp.path()).args(["send", &format!("{}/bin", server.base_url()), "--view", "csv"]).assert().failure().get_output().clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("isn't text"), "{}", String::from_utf8_lossy(&out.stderr));
}

/// Decision 10: an importer's answer is read like a HITOP JSON file, its
/// `workspace` is dropped, and a request file outside the folder is refused.
#[test]
fn an_importer_makes_collections_like_a_hitop_file() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    install(
        &home,
        &hooked(
            tmp.path(),
            "lines",
            "importer",
            "function importer(text, file) { return {id: 1, name: file, workspace: 'someone-else', requests: text.trim().split('\\n').map((u, i) => ({id: i + 1, name: 'r' + i, method: 'GET', url: u, headers: [], body: '', file: u.startsWith('..') ? u : undefined}))}; }",
        ),
    );
    fs::write(tmp.path().join("urls.txt"), "https://a.example/x\nhttps://b.example/y\n").unwrap();
    hitop(&home, tmp.path()).args(["import", "--plugin", "lines", "urls.txt", "--to", "out"]).assert().success();
    let yaml = fs::read_to_string(tmp.path().join("out/hitop.yaml")).unwrap();
    assert!(yaml.contains("name: urls.txt") && !yaml.contains("someone-else"), "{yaml}");
    let r = fs::read_to_string(tmp.path().join("out/requests/r1.yaml")).unwrap();
    assert!(r.contains("https://b.example/y"), "{r}");

    fs::write(tmp.path().join("bad.txt"), "../escape\n").unwrap();
    let out = hitop(&home, tmp.path()).args(["import", "--plugin", "lines", "bad.txt", "--to", "bad"]).assert().failure().get_output().clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid path"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!tmp.path().join("escape.yaml").exists() && !tmp.path().join("bad/escape.yaml").exists());

    // Into global storage: a clashing id and name never replace a collection.
    hitop(&home, tmp.path()).args(["import", "--plugin", "lines", "urls.txt"]).assert().success();
    hitop(&home, tmp.path()).args(["import", "--plugin", "lines", "urls.txt"]).assert().success().stdout(contains("Imported 'urls.txt (Imported)'"));
    let stored = fs::read_to_string(home.path().join("config/hitop/collections.json")).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
    let list = stored.as_array().unwrap();
    assert_eq!(list.len(), 2, "{stored}");
    assert_ne!(list[0]["id"], list[1]["id"]);
    assert!(!stored.to_string().contains("someone-else"));

    // A value plugin isn't an importer.
    install(&home, &plugin(tmp.path(), "v", "function value() { return 'x'; }"));
    let out = hitop(&home, tmp.path()).args(["import", "--plugin", "v", "urls.txt"]).assert().failure().get_output().clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("plugin 'v' has no importer hook"), "{}", String::from_utf8_lossy(&out.stderr));
}

/// Decision 4: a snippet plugin gets the request with secrets as {{NAME}};
/// --show-secrets is refused with a plugin.
#[test]
fn a_snippet_plugin_gets_placeholders_never_secrets() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    install(
        &home,
        &hooked(tmp.path(), "httpie", "snippet", "function snippet(r) { return ['http', r.method, r.url].concat(r.headers.map(h => h.name + ':' + JSON.stringify(h.value))).join(' ') + ' <<< ' + JSON.stringify(r.body.text) + ' # ' + r.placeholders.join(','); }"),
    );
    secret_api(tmp.path(), "https://api.example");
    let out = hitop(&home, tmp.path()).args(["snippet", "./api", "P", "--env", "ci", "--plugin", "httpie"]).env("HITOP_SECRET_CI_CRED", SECRET).assert().success().get_output().clone();
    let code = String::from_utf8_lossy(&out.stdout);
    assert_eq!(code, "http POST https://api.example/p Authorization:\"Bearer {{cred}}\" Content-Type:\"text/plain\" <<< \"hello\" # cred");
    assert!(String::from_utf8_lossy(&out.stderr).contains("secrets written as placeholders: {{cred}}"));
    let out = hitop(&home, tmp.path()).args(["snippet", "./api", "P", "--env", "ci", "--plugin", "httpie", "--show-secrets"]).env("HITOP_SECRET_CI_CRED", SECRET).assert().failure().get_output().clone();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--show-secrets can't be used with --plugin") && !err.contains(SECRET), "{err}");
}
