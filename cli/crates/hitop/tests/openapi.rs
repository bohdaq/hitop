//! `hitop import openapi` and `hitop openapi sync`: the Petstore fixture
//! imported from a file and from a URL (httpmock), run against a mock, and a
//! sync that keeps edits made to the files. Nothing reaches the internet.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn hitop(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .current_dir(home.path());
    c
}

fn petstore() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/openapi/petstore/openapi.yaml")
}

fn arg(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn yaml_files(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn imports_petstore_and_runs_it_against_a_mock() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("pets");
    hitop(&home)
        .args(["import", "openapi", arg(&petstore()), "--to", arg(&dir)])
        .assert()
        .success()
        .stdout(contains("Imported 'Swagger Petstore - OpenAPI 3.0'").and(contains("19 operations → 19 requests in 3 folders")).and(contains("petstore_auth: OAuth 2")));
    let files = yaml_files(&dir.join("requests"));
    assert_eq!(files.len(), 19);
    assert!(files.contains(&"pet/find-pet-by-id.yaml".to_string()), "{files:?}");
    assert_eq!(files.iter().filter(|f| f.starts_with("user/")).count(), 7);
    let get = fs::read_to_string(dir.join("requests/pet/find-pet-by-id.yaml")).unwrap();
    assert!(get.contains("url: ${baseUrl}/pet/${petId}\n") && get.contains("operationId: getPetById"), "{get}");

    // A second import into the same folder needs --force.
    hitop(&home).args(["import", "openapi", arg(&petstore()), "--to", arg(&dir)]).assert().failure().stderr(contains("--force"));
    // `hitop import <spec>` finds the spec by its content.
    hitop(&home).args(["import", arg(&petstore()), "--to", arg(&home.path().join("pets2"))]).assert().success().stdout(contains("19 requests"));

    let server = MockServer::start();
    let pet = server.mock(|when, then| {
        when.method(GET).path("/api/v3/pet/7").header("api_key", "k-1");
        then.status(200).json_body(serde_json::json!({"id": 7, "name": "Rex"}));
    });
    let any = server.mock(|_, then| {
        then.status(200).body("{}");
    });
    let base = format!("baseUrl={}", server.url("/api/v3"));
    hitop(&home)
        .args(["run", arg(&dir), "--var", &base, "--var", "petId=7", "--var", "api_key=k-1", "--var", "orderId=1", "--var", "username=ada"])
        .assert()
        .success();
    pet.assert_calls(1);
    assert!(any.calls() >= 17, "{}", any.calls());
}

#[test]
fn imports_from_a_url() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let text = fs::read_to_string(petstore()).unwrap();
    server.mock(|when, then| {
        when.method(GET).path("/openapi.yaml");
        then.status(200).header("content-type", "application/yaml").body(text);
    });
    server.mock(|when, then| {
        when.path("/missing.yaml");
        then.status(404);
    });
    let dir = home.path().join("pets");
    let url = server.url("/openapi.yaml");
    hitop(&home).args(["import", "openapi", &url, "--to", arg(&dir)]).assert().success().stdout(contains("19 requests"));
    let manifest = fs::read_to_string(dir.join("hitop.yaml")).unwrap();
    assert!(manifest.contains(&format!("source: {url}")), "{manifest}");
    hitop(&home).args(["import", "openapi", &server.url("/missing.yaml"), "--to", arg(&home.path().join("x"))]).assert().failure().stderr(contains("404"));
    // Into global storage without --to.
    hitop(&home).args(["import", "openapi", &url]).assert().success();
    hitop(&home).args(["collections", "list"]).assert().success().stdout(contains("Swagger Petstore"));
}

const V1: &str = r#"openapi: 3.0.3
info: {title: Shop, version: "1"}
servers: [{url: "https://shop.test/v1"}]
tags: [{name: pets}]
components:
  securitySchemes:
    key: {type: apiKey, in: header, name: X-Key}
security: [{key: []}]
paths:
  /pets:
    get:
      tags: [pets]
      operationId: listPets
      summary: List pets
      parameters:
      - {name: limit, in: query, required: true, example: 10}
      - {name: X-Api-Version, in: header, required: true, example: "1"}
      - {name: X-Client, in: header, required: true, example: cli}
      responses: {"200": {description: ok}}
    post:
      tags: [pets]
      operationId: addPet
      summary: Add a pet
      requestBody: {content: {application/json: {example: {name: Rex}}}}
      responses: {"201": {description: ok}}
  /pets/{id}:
    delete:
      tags: [pets]
      operationId: deletePet
      summary: Delete a pet
      parameters: [{name: id, in: path, required: true, schema: {type: integer}}]
      responses: {"204": {description: ok}}
"#;

fn v2() -> String {
    V1.replace("version: \"1\"", "version: \"2\"")
        .replace("example: 10}", "example: 20}")
        .replace("example: \"1\"}", "example: \"2\"}")
        .replace("example: {name: Rex}", "example: {name: Rex, kind: dog}")
        .replace(
            "  /pets/{id}:\n    delete:\n      tags: [pets]\n      operationId: deletePet\n      summary: Delete a pet\n",
            "  /pets/{id}:\n    get:\n      tags: [pets]\n      operationId: getPet\n      summary: Get a pet\n      parameters: [{name: id, in: path, required: true, schema: {type: integer}}]\n      responses: {\"200\": {description: ok}}\n    patch:\n      tags: [pets]\n      operationId: patchPet\n      summary: Patch a pet\n",
        )
}

#[test]
fn sync_keeps_edits_to_the_files() {
    let home = TempDir::new().unwrap();
    let spec = home.path().join("v1.yaml");
    fs::write(&spec, V1).unwrap();
    let dir = home.path().join("shop");
    hitop(&home).args(["import", "openapi", arg(&spec), "--to", arg(&dir)]).assert().success();
    let req = |p: &str| dir.join("requests").join(p);
    assert_eq!(yaml_files(&dir.join("requests")), ["pets/add-a-pet.yaml", "pets/delete-a-pet.yaml", "pets/list-pets.yaml"]);

    // Edits: a script and an assertion, an auth override, a header of your
    // own, a rename (name and file), and an environment.
    let list = fs::read_to_string(req("pets/list-pets.yaml")).unwrap();
    assert!(list.contains("headers:\n- name: X-Api-Version\n  value: '1'\n- name: X-Client\n  value: cli\n"), "{list}");
    let list = list.replace("name: List pets\n", "name: All my pets\n").replace("  value: cli\n", "  value: cli\n- name: X-Mine\n  value: mine\n").replace(
        "openapi:\n",
        "auth:\n  type: bearer\n  token: ${token}\nscripts:\n  pre: setHeader('X-Run', '1');\n  post: assert.status(200);\nopenapi:\n",
    );
    fs::create_dir_all(dir.join("requests/mine")).unwrap();
    fs::write(req("mine/all.yaml"), list).unwrap();
    fs::remove_file(req("pets/list-pets.yaml")).unwrap();
    let add = fs::read_to_string(req("pets/add-a-pet.yaml")).unwrap();
    // Your own body for addPet: the spec's new one must not replace it.
    let add = add.replace("\"name\": \"Rex\"", "\"name\": \"Fido\"");
    fs::write(req("pets/add-a-pet.yaml"), &add).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("environments/dev.yaml"), "variables:\n  token: t\n").unwrap();

    let spec2 = home.path().join("v2.yaml");
    fs::write(&spec2, v2()).unwrap();
    let before = yaml_files(&dir);
    hitop(&home)
        .args(["openapi", "sync", arg(&spec2), arg(&dir), "--dry-run"])
        .assert()
        .success()
        .stdout(
            contains("dry run")
                .and(contains("+ GET /pets/{id} (getPet)  Get a pet → requests/pets/get-a-pet.yaml"))
                .and(contains("~ GET /pets (listPets)  All my pets: updated url, headers\n"))
                // Row by row: the spec's change to one header, your own header.
                .and(contains("      ~ header X-Api-Version changed by the spec: updated\n"))
                .and(contains("      = header X-Mine added by you: kept\n"))
                .and(contains("~ POST /pets (addPet)  Add a pet: kept your body (the spec changed it too)"))
                .and(contains("- DELETE /pets/{id} (deletePet)  Delete a pet: removed from the spec: kept (--prune deletes it)"))
                .and(contains("2 added, 2 changed, 1 removed, 0 unchanged")),
        );
    assert_eq!(yaml_files(&dir), before, "a dry run writes nothing");

    hitop(&home).args(["openapi", "sync", arg(&spec2), arg(&dir)]).assert().success().stdout(contains("Updated"));
    let mine = fs::read_to_string(req("mine/all.yaml")).unwrap();
    assert!(mine.contains("name: All my pets\n") && mine.contains("- name: limit\n  value: '20'"), "{mine}");
    assert!(mine.contains("- name: X-Api-Version\n  value: '2'\n- name: X-Client\n  value: cli\n- name: X-Mine\n  value: mine\n"), "both outcomes: {mine}");
    assert!(mine.contains("pre: setHeader('X-Run', '1');") && mine.contains("post: assert.status(200);") && mine.contains("token: ${token}"), "{mine}");
    assert!(fs::read_to_string(req("pets/add-a-pet.yaml")).unwrap().contains("\"name\": \"Fido\""));
    assert!(req("pets/get-a-pet.yaml").is_file() && req("pets/patch-a-pet.yaml").is_file());
    assert!(req("pets/delete-a-pet.yaml").is_file(), "kept without --prune");
    assert!(!req("pets/list-pets.yaml").exists(), "the renamed file stays renamed");
    assert_eq!(fs::read_to_string(dir.join("environments/dev.yaml")).unwrap(), "variables:\n  token: t\n");
    assert!(fs::read_to_string(dir.join("hitop.yaml")).unwrap().contains("version: '2'"));

    hitop(&home).args(["openapi", "sync", arg(&spec2), arg(&dir)]).assert().success().stdout(contains("0 added, 0 changed, 1 removed, 4 unchanged"));
    hitop(&home).args(["openapi", "sync", arg(&spec2), arg(&dir), "--prune"]).assert().success().stdout(contains("deleted"));
    assert!(!req("pets/delete-a-pet.yaml").exists());

    // A folder that wasn't imported from a spec.
    let plain = home.path().join("plain");
    hitop(&home).args(["init", arg(&plain)]).assert().success();
    hitop(&home).args(["openapi", "sync", arg(&spec2), arg(&plain)]).assert().failure().stderr(contains("wasn't imported from an OpenAPI spec"));
}
