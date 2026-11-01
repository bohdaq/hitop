//! `hitop grpc` against a local tonic server (hitop-core's
//! tests/support/grpc_server.rs): unary and server-streaming calls, metadata,
//! an error status (exit 1), list and describe over reflection v1 and
//! v1alpha, .proto files instead of reflection, a saved request from a
//! collection folder with a masked secret, and `run` / `requests send` /
//! `snippet` leaving gRPC requests alone.

#[path = "../../hitop-core/tests/support/grpc_server.rs"]
mod grpc_server;

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use grpc_server::{fixtures, start, Reflect};
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).env("HITOP_KEYCHAIN", "off").current_dir(cwd);
    c
}

fn stdout(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap()
}

fn stderr(a: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(a.get_output().stderr.clone()).unwrap()
}

#[test]
fn unary_call_with_metadata() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::V1).to_string();
    let a = hitop(&home, tmp.path())
        .args(["grpc", &addr, "hitop.test.Greeter/SayHello", "-d", r#"{"name": "Ann"}"#, "-H", "x-tenant: acme", "--bearer", "tok", "--plaintext", "-v"])
        .assert()
        .success();
    let reply: Value = serde_json::from_str(&stdout(&a)).unwrap();
    assert_eq!((reply["message"].as_str(), reply["tenant"].as_str(), reply["authorization"].as_str()), (Some("Hello, Ann"), Some("acme"), Some("Bearer tok")));
    let err = stderr(&a);
    assert!(err.contains("Status: OK") && err.contains("x-served-by: hitop-test"), "{err}");
    // http:// is plaintext without the flag; the message can come from a file.
    fs::write(tmp.path().join("msg.json"), r#"{"name": "${who}"}"#).unwrap();
    let a = hitop(&home, tmp.path()).args(["grpc", &format!("http://{addr}"), "hitop.test.Greeter.SayHello", "-d", "@msg.json", "--var", "who=Bo"]).assert().success();
    assert!(stdout(&a).contains("Hello, Bo"), "{}", stdout(&a));
}

#[test]
fn server_streaming_as_json_lines() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::V1).to_string();
    let a = hitop(&home, tmp.path())
        .args(["grpc", &addr, "hitop.test.Greeter/StreamHellos", "--plaintext", "--json", "-d", "-"])
        .write_stdin(r#"{"name": "Cy", "times": 3}"#)
        .assert()
        .success();
    let lines: Vec<Value> = stdout(&a).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let events: Vec<&str> = lines.iter().map(|l| l["event"].as_str().unwrap()).collect();
    assert_eq!(events, ["headers", "message", "message", "message", "status"]);
    assert_eq!(lines[3]["data"]["message"], "Hello #2, Cy");
    assert_eq!(lines[3]["data"]["index"], 2);
    assert_eq!((lines[4]["code"].as_i64(), lines[4]["name"].as_str()), (Some(0), Some("OK")));
    assert!(lines[0]["time"].is_string());
}

#[test]
fn error_status_exits_1_with_trailers() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::V1).to_string();
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "hitop.test.Greeter/Fail", "-d", r#"{"name": "Zed"}"#, "--plaintext"]).assert().code(1);
    let err = stderr(&a);
    assert!(err.contains("Status: NOT_FOUND (5): no user named Zed") && err.contains("x-reason: test"), "{err}");
    assert!(stdout(&a).is_empty());
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "hitop.test.Greeter/Fail", "--plaintext", "--json"]).assert().code(1);
    let status: Value = serde_json::from_str(stdout(&a).lines().last().unwrap()).unwrap();
    assert_eq!((status["code"].as_i64(), status["name"].as_str(), status["trailers"]["x-reason"].as_str()), (Some(5), Some("NOT_FOUND"), Some("test")));
    // Nothing listening: UNAVAILABLE, exit 1.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
    hitop(&home, tmp.path()).args(["grpc", &closed, "a.B/C", "--plaintext", "--connect-timeout", "2000"]).assert().code(1).stderr(contains("UNAVAILABLE"));
    // Problems found before calling.
    hitop(&home, tmp.path()).args(["grpc", &addr, "hitop.test.Greeter/Chat", "--plaintext"]).assert().code(1).stderr(contains("bidirectional streaming"));
    hitop(&home, tmp.path()).args(["grpc", &addr, "hitop.test.Greeter/SayHello", "-d", "{nope", "--plaintext"]).assert().code(1).stderr(contains("isn't a valid hitop.test.HelloRequest"));
    hitop(&home, tmp.path()).args(["grpc", &addr, "SayHello", "--plaintext"]).assert().code(1).stderr(contains("package.Service/Method"));
}

#[test]
fn list_and_describe_over_reflection() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    for reflect in [Reflect::V1, Reflect::V1Alpha] {
        let addr = start(reflect).to_string();
        let a = hitop(&home, tmp.path()).args(["grpc", &addr, "list", "--plaintext"]).assert().success();
        assert!(stdout(&a).lines().any(|l| l == "hitop.test.Greeter"), "{}", stdout(&a));
        let a = hitop(&home, tmp.path()).args(["grpc", &addr, "list", "Greeter", "--plaintext"]).assert().success();
        assert_eq!(stdout(&a).lines().collect::<Vec<_>>(), ["hitop.test.Greeter.SayHello", "hitop.test.Greeter.StreamHellos", "hitop.test.Greeter.Fail", "hitop.test.Greeter.Chat"]);
    }
    let addr = start(Reflect::V1).to_string();
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "describe", "hitop.test.Greeter", "--plaintext"]).assert().success();
    assert!(stdout(&a).contains("rpc Chat(stream hitop.test.HelloRequest) returns (stream hitop.test.HelloReply);"), "{}", stdout(&a));
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "describe", "hitop.test.Greeter/SayHello", "--plaintext", "--json"]).assert().success();
    let v: Value = serde_json::from_str(&stdout(&a)).unwrap();
    assert_eq!(v["template"]["tag"]["key"], "");
    assert_eq!(v["template"]["mood"], "MOOD_UNSPECIFIED");
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "list", "--plaintext", "--json"]).assert().success();
    let v: Value = serde_json::from_str(&stdout(&a)).unwrap();
    let greeter = v.as_array().unwrap().iter().find(|s| s["name"] == "hitop.test.Greeter").unwrap();
    assert_eq!(greeter["methods"][1]["serverStreaming"], true);
    hitop(&home, tmp.path()).args(["grpc", &addr, "describe", "no.Such", "--plaintext"]).assert().code(1).stderr(contains("no service, method, message or enum"));
}

#[test]
fn proto_files_instead_of_reflection() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::None).to_string();
    hitop(&home, tmp.path()).args(["grpc", &addr, "list", "--plaintext"]).assert().code(1).stderr(contains("no reflection service"));
    let dir = fixtures();
    let proto = dir.join("greeter.proto");
    let include = dir.join("include");
    let (proto, include) = (proto.to_str().unwrap(), include.to_str().unwrap());
    hitop(&home, tmp.path()).args(["grpc", &addr, "list", "--plaintext", "--proto", proto]).assert().code(1).stderr(contains("common.proto"));
    let a = hitop(&home, tmp.path()).args(["grpc", &addr, "list", "--plaintext", "--proto", proto, "-I", include]).assert().success();
    assert!(stdout(&a).contains("hitop.test.Greeter"));
    let a = hitop(&home, tmp.path())
        .args(["grpc", &addr, "hitop.test.Greeter/SayHello", "--plaintext", "--proto", proto, "-I", include, "-d", r#"{"name": "Dee", "tag": {"key": "k"}}"#])
        .assert()
        .success();
    assert!(stdout(&a).contains("Hello, Dee"));
}

/// `api/`: a gRPC request with its .proto files in the folder (relative
/// paths), a secret bearer token and variables; plus an HTTP request.
fn folder(tmp: &TempDir, addr: &str) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::create_dir_all(dir.join("protos/include")).unwrap();
    fs::copy(fixtures().join("greeter.proto"), dir.join("protos/greeter.proto")).unwrap();
    fs::copy(fixtures().join("include/common.proto"), dir.join("protos/include/common.proto")).unwrap();
    fs::write(dir.join("hitop.yaml"), format!("format: 1\nname: Calls\nvariables:\n  host: {addr}\n  tenant: t9\n")).unwrap();
    fs::write(
        dir.join("requests/hello.yaml"),
        "name: Hello\nkind: grpc\nurl: ${host}\nheaders:\n- name: x-tenant\n  value: ${tenant}\nauth:\n  type: bearer\n  token: ${token}\ngrpc:\n  service: hitop.test.Greeter\n  method: SayHello\n  message: |\n    {\"name\": \"${tenant}\"}\n  protos:\n  - protos/greeter.proto\n  importPaths:\n  - protos/include\n  plaintext: true\n",
    )
    .unwrap();
    fs::write(dir.join("requests/health.yaml"), "name: Health\nurl: http://127.0.0.1:9/health\n").unwrap();
    fs::write(dir.join("environments/prod.yaml"), "name: prod\nvariables:\n  token:\n    secret: true\n").unwrap();
    dir
}

#[test]
fn a_saved_request_with_protos_and_a_masked_secret() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::None).to_string();
    let dir = folder(&tmp, &addr);
    let col = dir.to_str().unwrap();
    // Run from elsewhere: the .proto paths are the folder's.
    let a = hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "s3cr3t-tok").args(["grpc", "Hello", "-c", col, "--env", "prod", "-v"]).assert().success();
    let (out, err) = (stdout(&a), stderr(&a));
    assert!(!out.contains("s3cr3t") && !err.contains("s3cr3t"), "{out}{err}");
    let reply: Value = serde_json::from_str(&out).unwrap();
    // The server got the real token; hitop shows it masked.
    assert_eq!((reply["message"].as_str(), reply["tenant"].as_str(), reply["authorization"].as_str()), (Some("Hello, t9"), Some("t9"), Some("Bearer ••••")));
    // -d replaces the message, a method argument the method.
    let a = hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "x").args(["grpc", "Hello", "hitop.test.Greeter/StreamHellos", "-c", col, "--env", "prod", "-d", r#"{"times": 2}"#, "--json"]).assert().success();
    assert_eq!(stdout(&a).lines().filter(|l| l.contains("\"event\":\"message\"")).count(), 2);
    let a = hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "x").args(["grpc", "Hello", "list", "Greeter", "-c", col, "--env", "prod"]).assert().success();
    assert!(stdout(&a).contains("hitop.test.Greeter.Fail"));
    // An HTTP request isn't a gRPC one.
    hitop(&home, tmp.path()).args(["grpc", "Health", "-c", col]).assert().code(1).stderr(contains("hitop requests send"));
    hitop(&home, tmp.path()).args(["grpc", "Nope", "-c", col]).assert().code(1).stderr(contains("no gRPC request 'Nope'"));
}

#[test]
fn run_skips_and_send_and_snippet_refuse_grpc_requests() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let addr = start(Reflect::None).to_string();
    let dir = folder(&tmp, &addr);
    fs::remove_file(dir.join("requests/health.yaml")).unwrap();
    let col = dir.to_str().unwrap();
    hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "t").args(["run", col, "--env", "prod"]).assert().success();
    hitop(&home, tmp.path()).args(["requests", "send", col, "Hello"]).assert().failure().stderr(contains("hitop grpc"));
    hitop(&home, tmp.path()).args(["requests", "list", col]).assert().success().stdout(contains("GRPC"));
    hitop(&home, tmp.path()).args(["requests", "show", col, "Hello"]).assert().success().stdout(contains("hitop.test.Greeter/SayHello")).stdout(contains("protos/greeter.proto"));
    hitop(&home, tmp.path()).args(["snippet", col, "Hello", "--lang", "curl"]).assert().failure().stderr(contains("gRPC request"));
}
