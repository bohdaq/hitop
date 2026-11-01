//! GraphQL requests: `hitop send --graphql`, and saved `body.type: graphql`
//! requests through `hitop run`, `requests send` and a Postman import, all
//! against a local mock GraphQL endpoint (httpmock): a JSON POST of
//! `{query, variables, operationName}` with `${variables}` filled in.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::str::contains;
use serde_json::{json, Value};
use tempfile::TempDir;

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).env("HITOP_KEYCHAIN", "off").current_dir(cwd);
    c
}

const QUERY: &str = "query User($id: ID!) {\n  user(id: $id) { name }\n}\n";

/// The mock endpoint: answers `user` for the exact body a GraphQL client
/// sends, so a body that differs gets no match (404).
fn user_endpoint(server: &MockServer, variables: Value) -> httpmock::Mock<'_> {
    server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .header("content-type", "application/json")
            .json_body(json!({ "query": QUERY, "variables": variables, "operationName": "User" }));
        then.status(200).header("content-type", "application/json").json_body(json!({ "data": { "user": { "name": "Ada" } } }));
    })
}

/// A folder `api/` with `base` and `uid` variables and two GraphQL requests.
fn folder(tmp: &TempDir, base: &str) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::write(dir.join("hitop.yaml"), format!("format: 1\nname: Graph\nvariables:\n  base: {base}\n  uid: '42'\norder:\n- user\n- ping\n")).unwrap();
    fs::write(
        dir.join("requests/user.yaml"),
        "name: User\nmethod: POST\nurl: ${base}/graphql\nbody:\n  type: graphql\n  content: |\n    query User($id: ID!) {\n      user(id: $id) { name }\n    }\n  variables: |\n    {\"id\": \"${uid}\"}\n  operationName: User\nscripts:\n  post: |\n    assert.status(200);\n    assert.equal(getResponseValue('data.user.name'), 'Ada', 'name');\n",
    )
    .unwrap();
    // A Content-Type of the request's own wins over application/json.
    fs::write(
        dir.join("requests/ping.yaml"),
        "name: Ping\nmethod: POST\nurl: ${base}/graphql\nheaders:\n- name: Content-Type\n  value: application/graphql-response+json\nbody:\n  type: graphql\n  content: '{ ping }'\nscripts:\n  post: assert.status(200);\n",
    )
    .unwrap();
    dir
}

#[test]
fn send_a_query_from_a_file_with_variables() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let server = MockServer::start();
    let endpoint = user_endpoint(&server, json!({ "id": "42" }));
    let dir = folder(&tmp, &server.base_url());
    fs::write(tmp.path().join("user.graphql"), QUERY).unwrap();
    hitop(&home, tmp.path())
        .args(["send", &server.url("/graphql"), "--graphql", "user.graphql", "--graphql-vars", r#"{"id": "${uid}"}"#, "--operation-name", "User", "-c"])
        .arg(&dir)
        .assert()
        .success()
        .stdout(contains("Ada"));
    endpoint.assert();
}

#[test]
fn send_a_query_from_standard_input() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let server = MockServer::start();
    let endpoint = user_endpoint(&server, json!({ "id": "7" }));
    fs::write(tmp.path().join("vars.json"), "{\n  \"id\": \"7\"\n}\n").unwrap();
    hitop(&home, tmp.path())
        .args(["send", &server.url("/graphql"), "--graphql", "-", "--graphql-vars", "@vars.json", "--operation-name", "User", "--post-script", "assert.equal(getResponseValue('data.user.name'), 'Ada')"])
        .write_stdin(QUERY)
        .assert()
        .success();
    endpoint.assert();
}

#[test]
fn bad_variables_and_flag_conflicts() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let server = MockServer::start();
    let any = server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });
    hitop(&home, tmp.path())
        .args(["send", &server.url("/graphql"), "--graphql", "-", "--graphql-vars", "{id: 1}"])
        .write_stdin("{ me }")
        .assert()
        .failure()
        .stderr(contains("GraphQL variables are not valid JSON"));
    hitop(&home, tmp.path())
        .args(["send", &server.url("/graphql"), "--graphql", "-", "--graphql-vars", "[1]"])
        .write_stdin("{ me }")
        .assert()
        .failure()
        .stderr(contains("must be a JSON object"));
    hitop(&home, tmp.path()).args(["send", &server.url("/graphql"), "--graphql", "-", "-b", "x"]).assert().failure().stderr(contains("cannot be used with"));
    hitop(&home, tmp.path()).args(["send", &server.url("/graphql"), "--graphql-vars", "{}"]).assert().failure().stderr(contains("--graphql"));
    assert_eq!(any.calls(), 0, "nothing was sent");
}

#[test]
fn run_and_requests_send_a_saved_graphql_request() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let server = MockServer::start();
    let user = user_endpoint(&server, json!({ "id": "42" }));
    let ping = server.mock(|when, then| {
        when.method(POST).path("/graphql").header("content-type", "application/graphql-response+json").json_body(json!({ "query": "{ ping }" }));
        then.status(200).json_body(json!({ "data": { "ping": true } }));
    });
    let dir = folder(&tmp, &server.base_url());
    hitop(&home, tmp.path()).args(["run", "api"]).assert().success().stdout(contains("name"));
    user.assert_calls(1);
    ping.assert_calls(1);

    // A variable from the command line goes into the GraphQL variables.
    let other = user_endpoint(&server, json!({ "id": "9" }));
    hitop(&home, tmp.path()).args(["run", "api", "--var", "uid=9"]).assert().success();
    other.assert_calls(1);

    hitop(&home, tmp.path()).args(["requests", "send"]).arg(&dir).arg("User").assert().success().stdout(contains("Ada"));
    user.assert_calls(2);
}

#[test]
fn postman_graphql_imports_and_runs() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let server = MockServer::start();
    let endpoint = user_endpoint(&server, json!({ "id": "42" }));
    let collection = json!({
        "info": { "name": "Graph", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
        "variable": [{ "key": "base", "value": server.base_url() }, { "key": "uid", "value": "42" }],
        "item": [{ "name": "User", "request": { "method": "POST", "url": "{{base}}/graphql",
            "body": { "mode": "graphql", "graphql": { "query": QUERY, "variables": "{\"id\": \"{{uid}}\"}" } } },
            "event": [{ "listen": "test", "script": { "exec": ["pm.test('ok', () => pm.response.to.have.status(200));"] } }] }]
    });
    fs::write(tmp.path().join("graph.postman_collection.json"), collection.to_string()).unwrap();
    hitop(&home, tmp.path()).args(["import", "graph.postman_collection.json", "--to", "api"]).assert().success();
    let file = fs::read_to_string(tmp.path().join("api/requests/user.yaml")).unwrap();
    assert!(file.contains("type: graphql") && file.contains("variables: '{\"id\": \"${uid}\"}'"), "{file}");
    // Postman sends no operationName; the mock wants one, so add it to the file.
    fs::write(tmp.path().join("api/requests/user.yaml"), file.replace("  variables:", "  operationName: User\n  variables:")).unwrap();
    hitop(&home, tmp.path()).args(["run", "api"]).assert().success();
    endpoint.assert();
}
