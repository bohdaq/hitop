//! `hitop snippet`: the curl it prints, run against a local capture server,
//! sends the same request as `hitop requests send` (method, path, query,
//! headers, body; multipart boundaries aside), and secrets stay
//! placeholders unless `--show-secrets` is given.
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::mpsc;

use assert_cmd::Command;
use tempfile::TempDir;

fn hitop(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).env("HITOP_KEYCHAIN", "off").current_dir(home.path());
    c
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// A server that answers 200 to every request and hands over each one raw.
fn capture() -> (String, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut reader = BufReader::new(stream);
            let mut raw = Vec::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                raw.extend_from_slice(line.as_bytes());
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            raw.extend_from_slice(&body);
            let mut stream = reader.into_inner();
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            let _ = tx.send(raw);
        }
    });
    (base, rx)
}

/// Request line, headers (lower-case names, sorted, client-specific ones
/// left out) and body, with a multipart boundary replaced by `BOUNDARY`.
fn normalize(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap().to_string();
    let skip = ["host", "user-agent", "accept", "content-length", "accept-encoding", "connection", "accept-language", "sec-fetch-mode", "upgrade", "http2-settings"];
    let mut headers: Vec<String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    headers.sort();
    let mut out = format!("{request_line}\n{}\n\n{body}", headers.join("\n"));
    if let Some(b) = headers.iter().find_map(|h| h.strip_prefix("content-type: multipart/form-data; boundary=")) {
        out = out.replace(b, "BOUNDARY");
    }
    out
}

fn folder(home: &TempDir, base: &str) {
    let dir = home.path().join("api");
    write(&dir, "hitop.yaml", "format: 1\nname: Snip\n");
    write(&dir, "environments/dev.yaml", &format!("variables:\n  base: {base}\n  user: ada\n  token:\n    secret: true\n"));
    write(
        &dir,
        "requests/list.yaml",
        "name: List\nmethod: GET\nurl: ${base}/users?page=2&q=a%20b\nheaders:\n- name: X-Trace\n  value: t-1\n- name: X-Off\n  value: '1'\n  enabled: false\nauth:\n  type: bearer\n  token: ${token}\n",
    );
    write(
        &dir,
        "requests/create.yaml",
        "name: Create\nmethod: POST\nurl: ${base}/items\nheaders:\n- name: Content-Type\n  value: application/json\nauth:\n  type: basic\n  username: ${user}\n  password: p'w\"d $x\nbody:\n  type: json\n  content: |\n    {\"name\": \"it's \\\"quoted\\\"\", \"path\": \"C:\\\\tmp\", \"shell\": \"$HOME `id`\", \"city\": \"Kraków\"}\n",
    );
    write(
        &dir,
        "requests/profile.yaml",
        "name: Profile\nmethod: PUT\nurl: ${base}/profile\nauth:\n  type: apikey\n  key: api_key\n  value: k ${token}\n  in: query\nbody:\n  type: form\n  fields:\n  - name: name\n    value: Ada Lovelace\n  - name: note\n    value: a&b=c/é\n",
    );
    write(
        &dir,
        "requests/upload.yaml",
        "name: Upload\nmethod: POST\nurl: ${base}/upload\nauth:\n  type: apikey\n  key: X-API-Key\n  value: k-123\nbody:\n  type: multipart\n  fields:\n  - name: title\n    value: '@not-a-file'\n  - name: note\n    value: hi ${user}\n",
    );
    write(&dir, "requests/note.yaml", "name: Note\nmethod: PATCH\nurl: ${base}/note\nbody:\n  type: text\n  content: \"plain text\\twith 'quotes'\"\n");
    write(
        &dir,
        "requests/user.yaml",
        "name: User\nmethod: POST\nurl: ${base}/graphql\nbody:\n  type: graphql\n  content: |\n    query User($id: ID!) {\n      user(id: $id) { name }\n    }\n  variables: |\n    {\"id\": \"${user}\"}\n  operationName: User\n",
    );
    write(&dir, "requests/purge.yaml", "name: Purge\nmethod: PURGE\nurl: ${base}/assets/app.js\nheaders:\n- name: X-Tag\n  value: a\n- name: X-Tag\n  value: b\n");
}

/// Language, file name, how to run it, and the probe that tells the runtime is usable.
const RUNTIMES: &[(&str, &str, &[&str], &[&str])] = &[
    ("fetch", "snippet.mjs", &["node"], &["node", "--version"]),
    ("python", "snippet.py", &["python3"], &["python3", "-c", "import requests"]),
    ("java", "Main.java", &["java"], &["java", "-version"]),
    ("go", "main.go", &["go", "run"], &["go", "version"]),
];

const SECRET: &str = "s3cr3t-T0ken";

#[test]
fn generated_curl_sends_what_hitop_sends() {
    let home = TempDir::new().unwrap();
    let (base, rx) = capture();
    folder(&home, &base);
    for name in ["List", "Create", "Profile", "Upload", "Note", "Purge", "User"] {
        hitop(&home).args(["requests", "send", "./api", name, "--env", "dev", "--no-cookies"]).env("HITOP_SECRET_DEV_TOKEN", SECRET).assert().success();
        let sent = normalize(&rx.recv().unwrap());
        let out = hitop(&home).args(["snippet", "./api", name, "--env", "dev", "--lang", "curl", "--show-secrets"]).env("HITOP_SECRET_DEV_TOKEN", SECRET).assert().success();
        let code = String::from_utf8(out.get_output().stdout.clone()).unwrap();
        let run = std::process::Command::new("sh").arg("-c").arg(code.replace("curl ", "curl -sS -o /dev/null ")).output().unwrap();
        assert!(run.status.success(), "{name}: curl failed: {}\n{code}", String::from_utf8_lossy(&run.stderr));
        let got = normalize(&rx.recv().unwrap());
        assert_eq!(got, sent, "{name}: curl's request differs from hitop's\n{code}");

        // The snippets of other languages whose runtime is installed (CI has them all).
        for (lang, file, cmd, probe) in RUNTIMES {
            if !std::process::Command::new(probe[0]).args(&probe[1..]).output().is_ok_and(|o| o.status.success()) {
                continue;
            }
            let out = hitop(&home).args(["snippet", "./api", name, "--env", "dev", "--lang", lang, "--show-secrets"]).env("HITOP_SECRET_DEV_TOKEN", SECRET).assert().success();
            let dir = home.path().join(lang).join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(file), &out.get_output().stdout).unwrap();
            let run = std::process::Command::new(cmd[0]).args(&cmd[1..]).arg(file).current_dir(&dir).env("GOFLAGS", "-mod=mod").output().unwrap();
            assert!(run.status.success(), "{name}: {lang} failed: {}", String::from_utf8_lossy(&run.stderr));
            assert_eq!(normalize(&rx.recv().unwrap()), sent, "{name}: the {lang} snippet's request differs from hitop's");
        }
    }
}

#[test]
fn secrets_are_placeholders_by_default() {
    let home = TempDir::new().unwrap();
    folder(&home, "http://127.0.0.1:9");
    for lang in ["curl", "fetch", "python", "go", "java", "rust"] {
        let out = hitop(&home).args(["snippet", "./api", "List", "--env", "dev", "--lang", lang]).env("HITOP_SECRET_DEV_TOKEN", SECRET).assert().success();
        let code = String::from_utf8(out.get_output().stdout.clone()).unwrap();
        let err = String::from_utf8(out.get_output().stderr.clone()).unwrap();
        assert!(code.contains("Bearer {{token}}") && !code.contains(SECRET), "{lang}: {code}");
        assert!(code.contains("http://127.0.0.1:9/users?page=2&q=a%20b"), "{lang}: {code}");
        assert!(err.contains("secrets written as placeholders: {{token}}"), "{err}");
    }
    // The API key in the query keeps its placeholder readable.
    let out = hitop(&home).args(["snippet", "./api", "Profile", "-e", "dev"]).assert().success();
    assert!(String::from_utf8_lossy(&out.get_output().stdout).contains("?api_key=k%20{{token}}"));
    hitop(&home).args(["snippet", "./api", "List", "--lang", "cobol"]).assert().failure().stderr(predicates::str::contains("unknown language 'cobol'"));
    hitop(&home).args(["snippet", "./api", "Nope"]).assert().failure().stderr(predicates::str::contains("request 'Nope' not found"));
}
