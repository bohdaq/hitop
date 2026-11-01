//! The launch-gate scenario (ROADMAP T11): a Postman user imports a
//! collection and two environments, switches between them, logs in (the
//! token goes from the login response into the next requests through
//! `pm.environment.set` and the collection's Bearer auth) and gets a JUnit
//! report. Fixtures in cli/fixtures/launch-gate; CI runs the same scenario
//! through the container image and bohdaq/hitop-action (job `launch-gate`).
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::{json, Value};
use tempfile::TempDir;

const PASSWORDS: [(&str, &str); 2] = [("Staging", "staging-pw-7"), ("Local", "local-pw-8")];

fn hitop(home: &TempDir) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("HITOP_KEYCHAIN", "off")
        .current_dir(home.path());
    c
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/launch-gate").join(name)
}

fn arg(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// The API of one environment, like fixtures/launch-gate/mock_server.py.
fn api(env: &str, password: &str) -> MockServer {
    let server = MockServer::start();
    let token = format!("tok-{env}-42");
    let user = format!("ci-{env}");
    server.mock(|when, then| {
        when.method(POST)
            .path("/login")
            .header("content-type", "application/json")
            .json_body(json!({"username": user, "password": password}));
        then.status(200).json_body(json!({"token": token}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/orders").query_param("limit", "2").header("authorization", format!("Bearer {token}"));
        then.status(200).json_body(json!({"environment": env, "orders": [{"id": format!("ord-{env}-1")}, {"id": format!("ord-{env}-2")}]}));
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/orders/ord-{env}-1")).header("authorization", format!("Bearer {token}"));
        then.status(200).json_body(json!({"id": format!("ord-{env}-1")}));
    });
    server
}

/// Points an imported environment at a mock server.
fn point(dir: &Path, env_file: &str, server: &MockServer) {
    let path = dir.join("environments").join(env_file);
    let text = fs::read_to_string(&path).unwrap();
    let replaced = text.replace("http://127.0.0.1:8781", &server.base_url()).replace("http://127.0.0.1:8782", &server.base_url());
    assert_ne!(text, replaced, "{text}");
    fs::write(path, replaced).unwrap();
}

struct Case {
    name: String,
    file: Option<String>,
    failure: Option<String>,
    text: String,
}

/// The JUnit file: the suite's environment property and its testcases.
fn junit(path: &Path) -> (String, Vec<Case>, String) {
    let xml = fs::read_to_string(path).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let suites: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("testsuite")).collect();
    assert_eq!(suites.len(), 1, "{xml}");
    let env = suites[0]
        .descendants()
        .find(|n| n.has_tag_name("property") && n.attribute("name") == Some("environment"))
        .and_then(|n| n.attribute("value"))
        .unwrap_or_default()
        .to_string();
    let cases = suites[0]
        .children()
        .filter(|n| n.has_tag_name("testcase"))
        .map(|n| {
            let child = n.children().find(|c| c.is_element()).unwrap();
            Case {
                name: n.attribute("name").unwrap().into(),
                file: n.attribute("file").map(str::to_string),
                failure: (child.tag_name().name() != "system-out").then(|| child.attribute("message").unwrap_or_default().to_string()),
                text: child.text().unwrap_or_default().to_string(),
            }
        })
        .collect();
    (env, cases, xml)
}

#[test]
fn import_switch_environments_log_in_and_report_junit() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("shop");
    hitop(&home)
        .args([
            "import",
            arg(&fixture("shop.postman_collection.json")),
            arg(&fixture("staging.postman_environment.json")),
            arg(&fixture("local.postman_environment.json")),
            "--to",
            arg(&dir),
        ])
        .assert()
        .success()
        .stdout(contains("Imported 'Shop API'").and(contains("Imported environment 'Staging'")).and(contains("Imported environment 'Local'")));
    for f in ["requests/auth/log-in.yaml", "requests/orders/list-orders.yaml", "requests/orders/get-order.yaml"] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let staging_env = fs::read_to_string(dir.join("environments/staging.yaml")).unwrap();
    assert!(staging_env.contains("password:\n    secret: true"), "{staging_env}");

    let servers: Vec<MockServer> = PASSWORDS.iter().map(|(env, pw)| api(&env.to_lowercase(), pw)).collect();
    point(&dir, "staging.yaml", &servers[0]);
    point(&dir, "local.yaml", &servers[1]);

    for (i, (env, password)) in PASSWORDS.iter().enumerate() {
        let out = format!("reports/{}", env.to_lowercase());
        let mut c = hitop(&home);
        // The folder as a relative path, as in a repository: the `file`
        // attribute is that path plus requests/<file>.yaml.
        c.args(["run", "./shop", "--env", env, "--reporter", "cli,junit,json", "--out", &out]);
        // Staging gets the secret on the command line, Local from the env var.
        if i == 0 {
            c.args(["--secret", &format!("password={password}")]);
        } else {
            c.env(format!("HITOP_SECRET_{}_PASSWORD", env.to_uppercase()), password);
        }
        let output = c.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(0), "{env}\n{stdout}\n{}", String::from_utf8_lossy(&output.stderr));

        let (suite_env, cases, xml) = junit(&home.path().join(&out).join("junit.xml"));
        assert_eq!(suite_env, *env);
        let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Log in", "List orders", "Get order"]);
        assert!(cases.iter().all(|c| c.failure.is_none()), "{xml}");
        let files: Vec<&str> = cases.iter().map(|c| c.file.as_deref().unwrap()).collect();
        assert_eq!(files, ["shop/requests/auth/log-in.yaml", "shop/requests/orders/list-orders.yaml", "shop/requests/orders/get-order.yaml"]);
        for f in files {
            assert!(home.path().join(f).is_file(), "{f} exists");
        }
        // This environment's server answered, with the order List orders found.
        let base = servers[i].base_url();
        let lower = env.to_lowercase();
        assert!(cases[1].text.starts_with(&format!("GET {base}/orders?limit=2 -> 200")), "{}", cases[1].text);
        assert!(cases[2].text.starts_with(&format!("GET {base}/orders/ord-{lower}-1 -> 200")), "{}", cases[2].text);
        assert!(cases[1].text.contains("✓ orders from this environment"), "{}", cases[1].text);
        let report: Value = serde_json::from_str(&fs::read_to_string(home.path().join(&out).join("report.json")).unwrap()).unwrap();
        assert_eq!(report["summary"]["assertions"], json!({"total": 6, "passed": 6, "failed": 0}));
        assert_eq!(report["collections"][0]["iterations"][0]["requests"][0]["file"], "shop/requests/auth/log-in.yaml");
        for text in [xml.as_str(), &report.to_string(), &stdout] {
            assert!(!text.contains(password), "the password never reaches reports or output");
        }
    }

    // A wrong password: login fails, and the requests that need its token
    // fail too; the JUnit file says why.
    let out = home.path().join("reports/wrong");
    let output = hitop(&home)
        .args(["run", "shop", "--env", "Staging", "--secret", "password=nope", "--reporter", "junit", "--out", arg(&out)])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let (_, cases, xml) = junit(&out.join("junit.xml"));
    let login = cases[0].failure.as_deref().unwrap_or_default();
    assert!(login.starts_with("logged in: expected response to have status code 200 but got 404"), "{xml}");
    for case in &cases[1..] {
        assert_eq!(case.failure.as_deref(), Some("variable 'token' is not defined"), "{xml}");
    }
    assert_eq!(cases[0].file.as_deref(), Some("shop/requests/auth/log-in.yaml"));
}
