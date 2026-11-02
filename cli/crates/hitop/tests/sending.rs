//! What hitop adds when sending (spec/sending.md): a Content-Type for a raw
//! body with none, and a scheme for a URL typed without one, the same in
//! `hitop send`, `hitop run`, `hitop requests send` and `hitop snippet` as in
//! the web app and HitopKit. HTTP goes to a local mock server only.
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

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[test]
fn send_adds_the_content_type_of_a_raw_body() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let json = server.mock(|when, then| {
        when.method(POST).path("/json").header("content-type", "application/json").body("{\"a\": 1}");
        then.status(200);
    });
    let xml = server.mock(|when, then| {
        when.method(PUT).path("/xml").header("content-type", "application/xml");
        then.status(200);
    });
    let own = server.mock(|when, then| {
        when.method(POST).path("/own").header("content-type", "text/csv");
        then.status(200);
    });
    let get = server.mock(|when, then| {
        when.method(GET).path("/get").header_missing("content-type");
        then.status(200);
    });
    hitop(&home).args(["send", "-X", "POST", &server.url("/json"), "-b", "{\"a\": 1}"]).assert().success();
    hitop(&home).args(["send", "-X", "PUT", &server.url("/xml"), "-b", "<a/>"]).assert().success();
    hitop(&home).args(["send", "-X", "POST", &server.url("/own"), "-H", "Content-Type: text/csv", "-b", "{\"a\": 1}"]).assert().success();
    // Only POST, PUT and PATCH: a GET with a body gets none.
    hitop(&home).args(["send", &server.url("/get"), "-b", "{\"a\": 1}"]).assert().success();
    // The post-request script sees the header that was sent.
    let seen = server.mock(|when, then| {
        when.method(PATCH).path("/seen").header("content-type", "text/plain");
        then.status(200);
    });
    hitop(&home)
        .args(["send", "-X", "PATCH", &server.url("/seen"), "-b", "hi", "--post-script", "assert.equal(pm.request.headers.get('Content-Type'), 'text/plain', 'pm.request')"])
        .assert()
        .success()
        .stdout(contains("pm.request").and(contains("✗").not()));
    for m in [&json, &xml, &own, &get, &seen] {
        m.assert_calls(1);
    }
}

#[test]
fn send_gives_a_url_without_a_scheme_one() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let m = server.mock(|when, then| {
        when.method(GET).path("/x");
        then.status(200).body("plain http");
    });
    // 127.0.0.1:<port> is local: http://, as the web app sends it.
    let bare = server.address().to_string();
    hitop(&home).args(["send", "--raw", &format!("{bare}/x")]).assert().success().stdout(contains("plain http"));
    m.assert_calls(1);
    hitop(&home).args(["send", "users"]).assert().failure().stderr(contains("\"users\" isn't a URL. Enter a full URL, such as https://api.example.com/users"));
}

#[test]
fn run_requests_send_and_snippet_agree_on_the_content_type() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let dir = home.path().join("api");
    // `base` without a scheme: hitop adds http:// for 127.0.0.1.
    write(&dir, "hitop.yaml", "format: 1\nname: Bodies\norder:\n- json\n- html\n- own\n- form\n");
    write(&dir, "environments/mock.yaml", &format!("variables:\n  base: {}\n", server.address()));
    write(&dir, "requests/json.yaml", "name: Json\nmethod: post\nurl: ${base}/json\nbody:\n  type: text\n  content: '  [1, 2]'\nscripts:\n  post: assert.status(200);\n");
    write(&dir, "requests/html.yaml", "name: Html\nmethod: PUT\nurl: ${base}/html\nbody:\n  type: text\n  content: '<!DOCTYPE html><p>hi</p>'\nscripts:\n  post: assert.status(200);\n");
    write(
        &dir,
        "requests/own.yaml",
        "name: Own\nmethod: POST\nurl: ${base}/own\nheaders:\n- name: Content-Type\n  value: application/vnd.api+json\nbody:\n  type: json\n  content: '{}'\nscripts:\n  post: assert.status(200);\n",
    );
    // The script replaces a form body with text: that text says what it is.
    write(
        &dir,
        "requests/form.yaml",
        "name: Form\nmethod: POST\nurl: ${base}/form\nbody:\n  type: form\n  fields:\n  - name: a\n    value: '1'\nscripts:\n  pre: setBody('<x/>');\n  post: assert.status(200);\n",
    );
    let json = server.mock(|when, then| {
        when.method(POST).path("/json").header("content-type", "application/json");
        then.status(200);
    });
    let html = server.mock(|when, then| {
        when.method(PUT).path("/html").header("content-type", "text/html");
        then.status(200);
    });
    let own = server.mock(|when, then| {
        when.method(POST).path("/own").header("content-type", "application/vnd.api+json");
        then.status(200);
    });
    let form = server.mock(|when, then| {
        when.method(POST).path("/form").header("content-type", "application/xml").body("<x/>");
        then.status(200);
    });
    hitop(&home).args(["run", "api", "--env", "mock"]).assert().success();
    hitop(&home).args(["requests", "send", "api", "Json", "--env", "mock"]).assert().success();
    for (m, n) in [(&json, 2), (&html, 1), (&own, 1), (&form, 1)] {
        m.assert_calls(n);
    }
    // The snippet shows the header and the scheme hitop sends with.
    hitop(&home)
        .args(["snippet", "api", "Json", "--lang", "curl", "-e", "mock"])
        .assert()
        .success()
        .stdout(contains(format!("'http://{}/json'", server.address())).and(contains("-H 'Content-Type: application/json'")));
    hitop(&home).args(["snippet", "api", "Own", "--lang", "curl", "-e", "mock"]).assert().success().stdout(contains("application/vnd.api+json").and(contains("application/json'").not()));
    // Nothing of it is written to the collection.
    let saved = fs::read_to_string(dir.join("requests/json.yaml")).unwrap();
    assert!(!saved.contains("Content-Type") && saved.contains("url: ${base}/json"), "{saved}");
}
