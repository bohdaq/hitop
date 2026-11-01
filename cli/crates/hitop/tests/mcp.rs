//! `hitop mcp`: JSON-RPC over stdio against the real binary, every tool,
//! against a local mock server only. The secret test checks the whole stdout
//! stream, not only tool results.
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use httpmock::prelude::*;
use serde_json::{json, Value};
use tempfile::TempDir;

const SECRET: &str = "s3cr3t-T0ken";
const GLOBAL_SECRET: &str = "gl0bal-Pa55";

const TOOLS: [&str; 9] = [
    "create_request",
    "get_request",
    "list_collections",
    "list_environments",
    "list_requests",
    "run_collection",
    "run_request",
    "set_environment",
    "update_request",
];

struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Every line the server wrote to stdout.
    transcript: Vec<String>,
    next_id: u64,
}

impl Client {
    fn start(home: &TempDir, args: &[&str], env: &[(&str, &str)]) -> Client {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_hitop"));
        cmd.arg("mcp")
            .args(args)
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("HITOP_KEYCHAIN", "off")
            .current_dir(home.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut c = Client { child, stdin, lines, transcript: vec![], next_id: 1 };
        let init = c.request("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}));
        assert_eq!(init["result"]["serverInfo"]["name"], "hitop", "{init}");
        c.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        c
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self.lines.recv_timeout(Duration::from_secs(60)).expect("no answer from hitop mcp");
            self.transcript.push(line.clone());
            let v: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON on stdout ({e}): {line}"));
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    /// Calls a tool: (is_error, the text content parsed as JSON or as a string).
    fn call(&mut self, tool: &str, args: Value) -> (bool, Value) {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let result = &r["result"];
        assert!(result.is_object(), "{tool}: {r}");
        let text = result["content"][0]["text"].as_str().unwrap_or_default().to_string();
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (result["isError"] == json!(true), value)
    }

    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let (err, v) = self.call(tool, args);
        assert!(!err, "{tool} failed: {v}");
        v
    }

    fn err(&mut self, tool: &str, args: Value) -> String {
        let (err, v) = self.call(tool, args);
        assert!(err, "{tool} should fail: {v}");
        v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// A folder with variables in every scope, a secret environment variable
/// and a secret global.
fn folder(home: &TempDir, base: &str) -> std::path::PathBuf {
    let dir = home.path().join("api");
    write(&dir, "hitop.yaml", "format: 1\nname: Shop\nvariables:\n  b: C\n  c: C\n  d: C\norder:\n- whoami\n- echo\n- broken\n");
    write(&dir, "globals.yaml", "variables:\n  a: G\n  b: G\n  c: G\n  d: G\n  gpass:\n    secret: true\n");
    write(&dir, "environments/mock.yaml", &format!("variables:\n  base: {base}\n  c: E\n  d: E\n  token:\n    secret: true\n"));
    write(&dir, "environments/other.yaml", &format!("variables:\n  base: {base}\n"));
    write(
        &dir,
        "requests/whoami.yaml",
        "name: Who am I\nmethod: GET\nurl: ${base}/who?a=${a}&b=${b}&c=${c}&d=${d}&key=${token}\nheaders:\n- name: Authorization\n  value: Bearer ${token}\n- name: X-Global\n  value: ${gpass}\nscripts:\n  post: assert.status(200);\n",
    );
    // The response echoes the secret; the assertion fails with it in its message.
    write(
        &dir,
        "requests/echo.yaml",
        "name: Echo\nmethod: POST\nurl: ${base}/echo\nbody:\n  type: json\n  content: '{\"t\": \"${token}\"}'\nscripts:\n  post: assert.equal(pm.response.text(), 'something else', 'body was ' + pm.response.text());\n",
    );
    // Connection refused: the error names the URL with the secret in it.
    write(&dir, "requests/broken.yaml", "name: Broken\nmethod: GET\nurl: http://127.0.0.1:9/x?k=${token}\n");
    dir
}

#[test]
fn lists_exactly_the_tools_with_descriptions() {
    let home = TempDir::new().unwrap();
    let dir = folder(&home, "http://127.0.0.1:9");
    let mut c = Client::start(&home, &[dir.to_str().unwrap()], &[]);
    let r = c.request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, TOOLS);
    for t in tools {
        assert!(t["description"].as_str().is_some_and(|d| d.len() > 40), "{t}");
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }
    assert!(!names.iter().any(|n| n.contains("delete")));
}

#[test]
fn every_tool_on_a_folder_and_no_secret_in_any_output() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let who = server.mock(|when, then| {
        // globals < collection < environment < variables < secrets
        when.method(GET)
            .path("/who")
            .query_param("a", "G")
            .query_param("b", "C")
            .query_param("c", "E")
            .query_param("d", "V")
            .query_param("key", SECRET)
            .header("authorization", format!("Bearer {SECRET}"))
            .header("x-global", GLOBAL_SECRET);
        then.status(200).header("x-echo", SECRET).body(format!("hello {SECRET} and {GLOBAL_SECRET}"));
    });
    let echo = server.mock(|when, then| {
        when.method(POST).path("/echo");
        then.status(200).body(format!("token={SECRET}"));
    });
    let dir = folder(&home, &server.base_url());
    let mut c = Client::start(&home, &[dir.to_str().unwrap()], &[("HITOP_SECRET_MOCK_TOKEN", SECRET), ("HITOP_SECRET_GLOBAL_GPASS", GLOBAL_SECRET)]);

    let cols = c.ok("list_collections", json!({}));
    assert_eq!(cols["collections"], json!([{"name": "Shop", "requests": 3}]), "{cols}");

    let reqs = c.ok("list_requests", json!({}));
    let names: Vec<&str> = reqs["requests"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Who am I", "Echo", "Broken"]);
    assert!(c.err("list_requests", json!({"collection": "Other"})).contains("Shop"));

    let req = c.ok("get_request", json!({"request": "Who am I"}));
    assert_eq!(req["url"], "${base}/who?a=${a}&b=${b}&c=${c}&d=${d}&key=${token}");
    assert_eq!(req["headers"][0], json!({"name": "Authorization", "value": "Bearer ${token}", "enabled": true}));
    assert_eq!(c.ok("get_request", json!({"request": "whoami"}))["name"], "Who am I");
    assert!(c.err("get_request", json!({"request": "nope"})).contains("Who am I"));

    let envs = c.ok("list_environments", json!({}));
    assert_eq!(envs["active"], Value::Null);
    let mock = envs["environments"].as_array().unwrap().iter().find(|e| e["name"] == "mock").unwrap().clone();
    assert_eq!(mock["secrets"], json!(["token"]));
    assert!(mock["variables"].get("token").is_none(), "{mock}");
    assert_eq!(envs["globals"]["secrets"], json!(["gpass"]));

    assert!(c.err("set_environment", json!({"name": "staging"})).contains("mock, other"));
    assert_eq!(c.ok("set_environment", json!({"name": "mock"}))["active"], "mock");

    let res = c.ok("run_request", json!({"request": "Who am I", "variables": {"d": "V", "token": "not-the-secret"}}));
    assert_eq!(res["status"], 200, "{res}");
    assert_eq!(res["passed"], true, "{res}");
    assert_eq!(res["environment"], "mock");
    assert_eq!(res["body"], "hello •••• and ••••");
    assert_eq!(res["headers"]["x-echo"], "••••");
    assert!(res["url"].as_str().unwrap().ends_with("key=••••"), "{res}");
    who.assert();

    let report = c.ok("run_collection", json!({"variables": {"d": "V"}}));
    assert_eq!(report["reportVersion"], 1, "{report}");
    assert_eq!(report["passed"], false);
    assert_eq!(report["summary"]["requests"], json!({"total": 3, "passed": 1, "failed": 2}));
    let steps = &report["collections"][0]["iterations"][0]["requests"];
    assert_eq!(steps[1]["assertions"][0]["passed"], false);
    assert!(steps[1]["assertions"][0]["message"].as_str().unwrap().contains("••••"), "{steps}");
    assert!(steps[2]["error"].as_str().unwrap().contains("k=••••"), "{steps}");
    echo.assert();
    let bailed = c.ok("run_collection", json!({"variables": {"d": "nope"}, "bail": true}));
    assert_eq!(bailed["bailed"], true, "{bailed}");
    assert_eq!(bailed["summary"]["requests"]["total"], 1);

    // Writing a secret's value into a request is refused; ${token} is fine.
    let refused = c.err("create_request", json!({"name": "Leak", "url": "{{base}}/x", "headers": [{"name": "Authorization", "value": format!("Bearer {SECRET}")}]}));
    assert!(refused.contains("${name}"), "{refused}");
    assert!(c.err("update_request", json!({"request": "Echo", "body": format!("x{GLOBAL_SECRET}")})).contains("secret"));
    let created = c.ok(
        "create_request",
        json!({"name": "Ping", "method": "post", "url": "${base}/ping", "file": "health/ping", "auth": {"type": "bearer", "token": "${token}"}, "post_request_script": "assert.status(204);"}),
    );
    assert_eq!(created["method"], "POST");
    assert_eq!(created["file"], "health/ping");
    assert!(dir.join("requests/health/ping.yaml").is_file());
    assert!(c.err("create_request", json!({"name": "Ping", "url": "x"})).contains("already exists"));
    assert!(c.err("create_request", json!({"name": "Bad", "url": "x", "auth": {"type": "magic"}})).contains("bearer"));
    let ping = server.mock(|when, then| {
        when.method(POST).path("/ping").header("authorization", format!("Bearer {SECRET}"));
        then.status(204);
    });
    assert_eq!(c.ok("run_request", json!({"request": "health/ping"}))["passed"], true);
    ping.assert();

    let updated = c.ok("update_request", json!({"request": "Ping", "name": "Ping 2", "url": "${base}/ping2", "headers": [{"name": "X-A", "value": "1", "enabled": false}]}));
    assert_eq!(updated["name"], "Ping 2");
    assert_eq!(updated["url"], "${base}/ping2");
    assert_eq!(updated["headers"], json!([{"name": "X-A", "value": "1", "enabled": false}]));
    let yaml = fs::read_to_string(dir.join("requests/health/ping.yaml")).unwrap();
    assert!(yaml.contains("ping2") && yaml.contains("Ping 2"), "{yaml}");
    assert_eq!(c.ok("list_requests", json!({"collection": "Shop"}))["requests"].as_array().unwrap().len(), 4);

    assert_eq!(c.ok("set_environment", json!({}))["active"], Value::Null);
    // Without the environment, ${token} has no value: the error names it.
    let (_, r) = c.call("run_request", json!({"request": "Echo"}));
    assert!(r.to_string().contains("token") || r.to_string().contains("base"), "{r}");

    let all = c.transcript.join("\n");
    assert!(!all.contains(SECRET) && !all.contains(GLOBAL_SECRET), "a secret value reached stdout:\n{all}");
    for f in walk(&dir) {
        let text = fs::read_to_string(&f).unwrap();
        assert!(!text.contains(SECRET) && !text.contains(GLOBAL_SECRET), "{}", f.display());
    }
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn a_missing_secret_is_a_tool_error_and_the_server_keeps_going() {
    let home = TempDir::new().unwrap();
    let dir = folder(&home, "http://127.0.0.1:9");
    let mut c = Client::start(&home, &[dir.to_str().unwrap()], &[("HITOP_SECRET_GLOBAL_GPASS", GLOBAL_SECRET)]);
    c.ok("set_environment", json!({"name": "mock"}));
    let e = c.err("run_request", json!({"request": "Who am I"}));
    assert!(e.contains("HITOP_SECRET_MOCK_TOKEN") && e.contains("hitop secret set token --env mock"), "{e}");
    assert!(!e.contains("--secret"), "{e}");
    let e = c.err("run_collection", json!({}));
    assert!(e.contains("HITOP_SECRET_MOCK_TOKEN"), "{e}");
    // Another environment, given for one call only, works.
    let r = c.ok("run_request", json!({"request": "Broken", "environment": "other"}));
    assert_eq!(r["passed"], false);
    assert!(r["error"].is_string(), "{r}");
    assert_eq!(c.ok("list_collections", json!({}))["collections"][0]["name"], "Shop");
    assert!(!c.transcript.join("\n").contains(GLOBAL_SECRET));
}

#[test]
fn websocket_requests_are_shown_but_not_run() {
    let home = TempDir::new().unwrap();
    let dir = folder(&home, "http://127.0.0.1:9");
    write(&dir, "requests/feed.yaml", "name: Feed\nkind: websocket\nurl: ws://127.0.0.1:9/feed\nwebsocket:\n  subprotocols: [v1]\n  messages:\n  - name: hi\n    content: hello\n");
    let mut c = Client::start(&home, &[dir.to_str().unwrap()], &[("HITOP_SECRET_GLOBAL_GPASS", GLOBAL_SECRET)]);
    let r = c.ok("get_request", json!({"request": "Feed"}));
    assert_eq!(r["kind"], "websocket");
    assert_eq!(r["websocket"], json!({"subprotocols": ["v1"], "messages": [{"name": "hi", "content": "hello"}]}));
    let list = c.ok("list_requests", json!({}));
    assert!(list["requests"].as_array().unwrap().iter().any(|x| x["name"] == "Feed" && x["kind"] == "websocket"), "{list}");
    let e = c.err("run_request", json!({"request": "Feed", "environment": "other"}));
    assert!(e.contains("WebSocket request") && e.contains("hitop ws"), "{e}");
}

#[test]
fn grpc_requests_are_shown_but_not_run() {
    let home = TempDir::new().unwrap();
    let dir = folder(&home, "http://127.0.0.1:9");
    write(&dir, "requests/hello.yaml", "name: Hello\nkind: grpc\nurl: 127.0.0.1:9\ngrpc:\n  service: a.Greeter\n  method: SayHello\n  message: '{}'\n  plaintext: true\n");
    let mut c = Client::start(&home, &[dir.to_str().unwrap()], &[("HITOP_SECRET_GLOBAL_GPASS", GLOBAL_SECRET)]);
    let r = c.ok("get_request", json!({"request": "Hello"}));
    assert_eq!(r["kind"], "grpc");
    assert_eq!(r["grpc"], json!({"service": "a.Greeter", "method": "SayHello", "message": "{}", "protos": [], "importPaths": [], "plaintext": true}));
    let e = c.err("run_request", json!({"request": "Hello", "environment": "other"}));
    assert!(e.contains("gRPC request") && e.contains("hitop grpc"), "{e}");
}

#[test]
fn global_storage_active_environment_and_cookies_across_calls() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let config = home.path().join("config/hitop");
    fs::create_dir_all(&config).unwrap();
    let collections = json!([
        {"id": 1, "name": "Site", "variables": {}, "requests": [
            {"id": 2, "name": "Login", "url": "${base}/login", "method": "POST", "headers": [], "body": ""},
            {"id": 3, "name": "Me", "url": "${base}/me", "method": "GET", "headers": [], "body": "", "postRequestScript": "assert.status(200);"}
        ]},
        {"id": 4, "name": "Other", "variables": {}, "requests": []}
    ]);
    fs::write(config.join("collections.json"), collections.to_string()).unwrap();
    let envs = json!({"active": "e1", "environments": [{"id": "e1", "name": "local", "variables": {"base": server.base_url()}}, {"id": "e2", "name": "prod", "variables": {}}]});
    fs::write(config.join("environments.json"), envs.to_string()).unwrap();
    // A folder named like a collection in the working directory must not win.
    write(&home.path().join("Site"), "hitop.yaml", "format: 1\nname: Decoy\n");

    let login = server.mock(|when, then| {
        when.method(POST).path("/login");
        then.status(200).header("set-cookie", "sid=abc123; Path=/");
    });
    let me = server.mock(|when, then| {
        when.method(GET).path("/me").header("cookie", "sid=abc123");
        then.status(200).body("me");
    });

    let mut c = Client::start(&home, &[], &[]);
    let cols = c.ok("list_collections", json!({}));
    assert_eq!(cols["workspace"], "global storage");
    assert_eq!(cols["collections"], json!([{"name": "Site", "requests": 2}, {"name": "Other", "requests": 0}]));
    assert!(c.err("list_requests", json!({})).contains("Site, Other"));
    assert_eq!(c.ok("list_requests", json!({"collection": "Site"}))["requests"][1]["name"], "Me");
    let envs = c.ok("list_environments", json!({}));
    assert_eq!(envs["active"], "local", "{envs}");
    assert_eq!(envs["environments"][0]["active"], true);

    assert_eq!(c.ok("run_request", json!({"collection": "Site", "request": "Login"}))["status"], 200);
    // The cookie from the first call is sent with the second, as in `hitop run`.
    let r = c.ok("run_request", json!({"collection": "Site", "request": "Me"}));
    assert_eq!(r["passed"], true, "{r}");
    login.assert();
    me.assert();
    let report = c.ok("run_collection", json!({"collection": "Site", "iterations": 2}));
    assert_eq!(report["summary"]["iterations"], 2, "{report}");
    assert_eq!(report["passed"], true, "{report}");
    assert!(c.err("run_collection", json!({"collection": "Site", "iterations": 0})).contains("iterations"));

    let created = c.ok("create_request", json!({"collection": "Other", "name": "Health", "url": "${base}/health"}));
    assert_eq!(created["collection"], "Other");
    assert!(c.err("create_request", json!({"collection": "Other", "name": "F", "url": "x", "file": "a/b"})).contains("folder"));
    c.ok("update_request", json!({"collection": "Other", "request": "Health", "method": "HEAD"}));
    let saved: Value = serde_json::from_str(&fs::read_to_string(config.join("collections.json")).unwrap()).unwrap();
    assert_eq!(saved[1]["requests"][0]["method"], "HEAD");
    // set_environment is for the session only.
    assert_eq!(c.ok("set_environment", json!({"name": "prod"}))["active"], "prod");
    let on_disk: Value = serde_json::from_str(&fs::read_to_string(config.join("environments.json")).unwrap()).unwrap();
    assert_eq!(on_disk["active"], "e1");
}

#[test]
fn a_path_that_is_no_folder_fails_to_start() {
    let home = TempDir::new().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hitop"))
        .args(["mcp", "./missing"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .current_dir(home.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("hitop init"));
    assert!(out.stdout.is_empty());
}
