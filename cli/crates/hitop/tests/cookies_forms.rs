//! The cookie jar across requests (`hitop send`, `hitop run`, `hitop
//! cookies`), and form bodies (`-F`, `--urlencoded`, saved form requests).
//! HTTP goes to a local mock server only.
use std::fs;
use std::path::Path;

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn hitop(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).env("HITOP_KEYCHAIN", "off").current_dir(home.path());
    c
}

#[test]
fn send_keeps_cookies_between_requests() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let login = server.mock(|when, then| {
        when.method(POST).path("/login");
        then.status(200).header("set-cookie", "sid=abc; Path=/; HttpOnly").header("set-cookie", "theme=dark; Path=/; Max-Age=3600").body("ok");
    });
    let me = server.mock(|when, then| {
        when.method(GET).path("/me").header("cookie", "sid=abc; theme=dark");
        then.status(200).body("Ada");
    });
    let anonymous = server.mock(|when, then| {
        when.method(GET).path("/me").header_missing("cookie");
        then.status(401).body("who?");
    });

    hitop(&home).args(["send", "-X", "POST", &server.url("/login")]).assert().success();
    hitop(&home).args(["send", "--raw", &server.url("/me")]).assert().success().stdout(contains("Ada"));
    hitop(&home).args(["cookies", "ls"]).assert().success().stdout(contains("sid=abc").and(contains("127.0.0.1")).and(contains("theme=dark")));
    // --no-cookies neither sends nor keeps them.
    hitop(&home).args(["send", "--raw", "--no-cookies", &server.url("/me")]).assert().success().stdout(contains("who?"));
    hitop(&home).args(["cookies", "clear"]).assert().success().stdout(contains("Deleted 2"));
    hitop(&home).args(["send", "--raw", &server.url("/me")]).assert().success().stdout(contains("who?"));
    login.assert_calls(1);
    me.assert_calls(1);
    anonymous.assert_calls(2);

    // A request's own Cookie header wins over the jar.
    hitop(&home).args(["send", "-X", "POST", &server.url("/login")]).assert().success();
    let own = server.mock(|when, then| {
        when.method(GET).path("/own").header("cookie", "mine=1");
        then.status(200);
    });
    hitop(&home).args(["send", "-H", "Cookie: mine=1", &server.url("/own")]).assert().success();
    own.assert_calls(1);
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[test]
fn run_sends_cookies_and_form_bodies() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let dir = home.path().join("api");
    write(&dir, "hitop.yaml", "format: 1\nname: Cookie API\norder:\n- login\n- me\n- upload\n- search\n");
    write(&dir, "environments/mock.yaml", &format!("variables:\n  base: {}\n  who: Ada\n", server.base_url()));
    write(&dir, "requests/login.yaml", "name: Login\nmethod: POST\nurl: ${base}/login\nbody:\n  type: form\n  fields:\n  - name: user\n    value: ${who}\n  - name: skip\n    value: x\n    enabled: false\nscripts:\n  post: assert.status(200);\n");
    write(&dir, "requests/me.yaml", "name: Me\nmethod: GET\nurl: ${base}/me\nscripts:\n  post: assert.status(200);\n");
    write(&dir, "requests/upload.yaml", "name: Upload\nmethod: POST\nurl: ${base}/upload\nbody:\n  type: multipart\n  fields:\n  - name: note\n    value: hi ${who}\nscripts:\n  post: assert.status(201);\n");
    write(&dir, "requests/search.yaml", "name: Search\nmethod: POST\nurl: ${base}/search\nheaders:\n- name: Content-Type\n  value: application/json\nbody:\n  type: json\n  content: '{\"q\": 1}'\nscripts:\n  post: assert.status(200);\n");

    let login = server.mock(|when, then| {
        when.method(POST).path("/login").header("content-type", "application/x-www-form-urlencoded").body("user=Ada");
        then.status(200).header("set-cookie", "sid=s1; Path=/");
    });
    let me = server.mock(|when, then| {
        when.method(GET).path("/me").header("cookie", "sid=s1");
        then.status(200);
    });
    let upload = server.mock(|when, then| {
        when.method(POST)
            .path("/upload")
            .header("cookie", "sid=s1")
            .header_matches("content-type", "^multipart/form-data; boundary=")
            .body_includes("Content-Disposition: form-data; name=\"note\"\r\n\r\nhi Ada\r\n");
        then.status(201);
    });
    let search = server.mock(|when, then| {
        when.method(POST).path("/search").header("content-type", "application/json").body("{\"q\": 1}");
        then.status(200);
    });

    hitop(&home).args(["run", "./api", "--env", "mock"]).assert().success().stdout(contains("4/4 requests succeeded"));
    login.assert();
    me.assert();
    upload.assert();
    search.assert();
    // The folder's jar stays for the next run.
    hitop(&home).args(["cookies", "ls", "-c", "./api"]).assert().success().stdout(contains("sid=s1"));
    hitop(&home).args(["cookies", "ls"]).assert().success().stdout(contains("No cookies"));
    // --no-cookies: Me goes out without the cookie and fails.
    hitop(&home).args(["run", "./api", "--env", "mock", "--no-cookies"]).assert().code(1);
    me.assert_calls(1);
}

#[test]
fn send_uploads_files_with_dash_f() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let file = home.path().join("report.csv");
    fs::write(&file, "id,name\n1,Ada é\n").unwrap();
    let blob = home.path().join("blob.bin");
    fs::write(&blob, [0u8, 159, 146, 150, 255]).unwrap();
    let upload = server.mock(|when, then| {
        when.method(POST)
            .path("/upload")
            .header_matches("content-type", "^multipart/form-data; boundary=----hitop[0-9a-f]{32}$")
            .body_includes("Content-Disposition: form-data; name=\"title\"\r\n\r\nQ3 for Ada\r\n")
            .body_includes("Content-Disposition: form-data; name=\"file\"; filename=\"report.csv\"\r\nContent-Type: text/csv\r\n\r\nid,name\n1,Ada é\n\r\n")
            .body_includes("name=\"raw\"; filename=\"blob.bin\"\r\nContent-Type: application/x-thing\r\n\r\n")
            .is_true(|req| req.body().to_vec().windows(5).any(|w| w == [0u8, 159, 146, 150, 255]));
        then.status(201).body("stored");
    });
    hitop(&home)
        .args(["send", "--raw", &server.url("/upload"), "-F", "title=Q3 for Ada", "-F"])
        .arg(format!("file=@{}", file.display()))
        .arg("-F")
        .arg(format!("raw=@{};type=application/x-thing", blob.display()))
        .assert()
        .success()
        .stdout(contains("stored"));
    upload.assert();

    let form = server.mock(|when, then| {
        when.method(PUT).path("/form").header("content-type", "application/x-www-form-urlencoded").body("a%20b=1%262&c=");
        then.status(200);
    });
    hitop(&home).args(["send", "-X", "PUT", &server.url("/form"), "-F", "a b=1&2", "-F", "c=", "--urlencoded"]).assert().success();
    form.assert();
    hitop(&home).args(["send", &server.url("/form"), "-F", "f=@nope.txt"]).assert().failure().stderr(contains("reading nope.txt"));
    hitop(&home).arg("send").arg(server.url("/form")).arg("-F").arg(format!("f=@{}", file.display())).arg("--urlencoded").assert().failure().stderr(contains("can't send files"));
    hitop(&home).args(["send", &server.url("/form"), "-F", "novalue"]).assert().failure().stderr(contains("NAME=VALUE"));
}
