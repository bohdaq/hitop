//! `hitop import` of Insomnia, Bruno and HAR files (fixtures/imports): into a folder, then read back
//! by hitop itself.
use std::path::PathBuf;

use assert_cmd::Command;
use hitop_core::format::{read_folder, Folder};
use hitop_core::model::{Auth, Request};
use tempfile::TempDir;

fn fixture(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/imports").join(p)
}

fn import(p: &str) -> (Folder, String) {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("out");
    let a = Command::cargo_bin("hitop")
        .unwrap()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .args(["import", fixture(p).to_str().unwrap(), "--to", dir.to_str().unwrap()])
        .assert()
        .success();
    let o = a.get_output();
    (read_folder(&dir).unwrap(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

fn get<'a>(f: &'a Folder, name: &str) -> &'a Request {
    f.collection.requests.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("{name} in {:?}", f.collection.requests.iter().map(|r| &r.name).collect::<Vec<_>>()))
}

#[test]
fn insomnia_v5() {
    let (f, out) = import("insomnia/auth-types.yaml");
    assert_eq!(f.collection.name, "Auth tests");
    let basic = get(&f, "sends request with basic authentication");
    assert_eq!(basic.auth, Some(Auth::Basic { username: "user".into(), password: "pass".into() }));
    assert!(!basic.headers[0].enabled);
    assert!(out.contains("oauth1 auth"), "{out}");
    assert_eq!(f.collection.variables["customValue"], "fromEnvManager");

    let (f, _) = import("insomnia/after-response-collection.yaml");
    let r = get(&f, "tests with expect and test");
    assert!(r.post_request_script.contains("pm.test('happy tests'") && !r.post_request_script.contains("insomnia."), "{}", r.post_request_script);

    let (f, _) = import("insomnia/environments.yaml");
    assert!(get(&f, "New Request").body.contains("${exampleObject.anotherString}\n${exampleString}"));
    assert_eq!(f.collection.variables["exampleObject.anotherString"], "baseenv1");
    assert!(f.environments.iter().any(|e| e.name == "ExampleA" && e.variables["exampleString"].starts_with("sube")));

    let (f, _) = import("insomnia/graphql.yaml");
    let r = get(&f, "GraphQL request with variables");
    assert!(r.body.starts_with("query($vars: VarsInput)") && r.graphql.as_ref().unwrap().variables.contains("stringVar"), "{r:?}");

    let (f, out) = import("insomnia/environment-cascade-collection.yaml");
    assert_eq!(get(&f, "inner request").file.as_deref(), Some("cascadefolder/innerfolder/inner-request"));
    assert!(out.contains("a folder variable"), "{out}");
}

#[test]
fn insomnia_v4() {
    let (f, out) = import("insomnia/insomnia_v4.json");
    assert_eq!(f.collection.requests.len(), 13);
    assert_eq!(get(&f, "Add a new pet to the store").url, "${base_url}/pet");
    assert_eq!(f.collection.variables["base_url"], "${scheme}://${host}${base_path}");
    assert_eq!(f.environments[0].variables["host"], "petstore.swagger.io");
    assert!(get(&f, "Example 2").file.as_deref().unwrap().starts_with("example-folder/"));
    assert!(out.contains("unit test suites"), "{out}");
}

#[test]
fn bruno_folder() {
    let (f, out) = import("bruno/testbench");
    let c = &f.collection;
    assert_eq!(c.name, "bruno-testbench");
    assert_eq!(c.auth, Some(Auth::Bearer { token: "${bearer_auth_token}".into() }));
    assert_eq!(c.variables["collection-var"], "collection-var-value");
    assert_eq!(c.variables["coll_num"], "100");
    let r = get(&f, "echo json");
    assert_eq!((r.method.as_str(), r.url.as_str(), r.auth.as_ref()), ("POST", "${host}/api/echo/json", Some(&Auth::None)));
    assert_eq!(r.body, "{\n  \"hello\": \"bruno\"\n}");
    let on: Vec<(&str, &str)> = r.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect();
    assert!(on.contains(&("foo", "bar")) && on.contains(&("check", "again")) && on.contains(&("Content-Type", "application/json")), "{on:?}");
    assert!(r.post_request_script.contains("pm.test(\"status eq 200\""), "{}", r.post_request_script);
    assert!(r.post_request_script.contains("// test(\"should return json\"") && out.contains("Bruno's script API"), "{out}");
    assert_eq!(get(&f, "inherit Bearer Auth 200").auth, None, "the collection's auth");
    let form = get(&f, "echo form-url-encoded");
    assert_eq!(form.form_fields.iter().filter(|x| x.key == "key_1").count(), 2);
    let multi = get(&f, "echo multipart");
    let file = multi.form_fields.iter().find(|x| x.key == "file").unwrap();
    assert_eq!((file.kind.as_str(), file.value.as_str()), ("file", ""));
    assert_eq!(multi.form_fields[0].value, "{\"bar\":\"baz\"}");
    assert_eq!(multi.form_fields.iter().find(|x| x.key == "multiline").unwrap().value.trim(), "\"multiline-test\"");
    assert_eq!(get(&f, "variables interpolation").body.trim(), "query { __typename }");
    assert!(get(&f, "test-assert-combinations").post_request_script.contains(r#"pm.response.json().numberAsString).to.eql("123")"#));
    let local = f.environments.iter().find(|e| e.name == "Local").unwrap();
    assert!(local.secrets.contains(&"github_client_secret".to_string()), "{:?}", local.secrets);
    assert_eq!(f.environments.iter().find(|e| e.name == "Prod").unwrap().variables["typed_env_num"], "300");
    assert!(out.contains("process.env") && out.contains("file body: attach the file again"), "{out}");
}

#[test]
fn har_without_its_credentials() {
    let (f, out) = import("har/recording.har");
    let names: Vec<(&str, &str)> = f.collection.requests.iter().map(|r| (r.file.as_deref().unwrap(), r.name.as_str())).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    let r = get(&f, "GET /orders");
    assert_eq!(r.url, "https://shop.test/orders?page=1");
    assert_eq!(r.headers.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["Accept"]);
    assert_eq!(get(&f, "POST /login").form_fields[0].value, "1 2");
    assert_eq!(get(&f, "POST /orders").body, "{\"id\": 1}");
    assert!(out.contains("Authorization header, left out") && out.contains("Cookie header, left out") && out.contains("the same request again"), "{out}");
    assert!(!out.contains("recorded-s3cret") && !format!("{:?}", f.collection).contains("recorded-s3cret"));
}

#[test]
fn refusals_say_why() {
    let home = TempDir::new().unwrap();
    let run = |name: &str, text: &str, why: &str| {
        let p = home.path().join(name);
        std::fs::write(&p, text).unwrap();
        let a = Command::cargo_bin("hitop").unwrap().env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("c")).args(["import", p.to_str().unwrap()]).assert().failure();
        let e = String::from_utf8_lossy(&a.get_output().stderr).into_owned();
        assert!(e.contains(why), "{e}");
    };
    run("v3.json", r#"{"_type":"export","__export_format":3,"resources":[]}"#, "format 3");
    run("mock.yaml", "type: mock.insomnia.rest/5.0\nname: x\n", "mock.insomnia.rest");
    run("empty.har", r#"{"log":{"entries":[]}}"#, "no http(s) requests");
    // A cycle of parents ends.
    run("loop.json", r#"{"_type":"export","__export_format":4,"resources":[{"_id":"a","parentId":"b","_type":"request_group"},{"_id":"b","parentId":"a","_type":"request_group"}]}"#, "no requests");
}
