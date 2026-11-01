//! The globals scope: below collection variables and environments, secret
//! globals (HITOP_SECRET_GLOBAL_<NAME>), pm.globals in scripts, the `hitop
//! globals` commands, Postman globals import and `convert --globals`. HTTP
//! goes to a local mock server only.
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
fn folder_globals_sit_below_collection_and_environment() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    let dir = home.path().join("api");
    write(&dir, "hitop.yaml", "format: 1\nname: G\nvariables:\n  who: C\norder:\n- first\n- second\n");
    write(&dir, "globals.yaml", "variables:\n  who: G\n  k: G\n  only: G\n  y: G\n  token:\n    secret: true\n");
    write(&dir, "environments/mock.yaml", &format!("variables:\n  base: {}\n  k: E\n  y: E\n", server.base_url()));
    write(
        &dir,
        "requests/first.yaml",
        "name: First\nmethod: GET\nurl: ${base}/first?who=${who}&k=${k}&only=${only}&x=${x}\nheaders:\n- name: Authorization\n  value: Bearer ${token}\nscripts:\n  pre: |\n    pm.globals.set('x', 'from-script'); pm.globals.set('y', 'G2');\n  post: |\n    pm.test('globals scope', () => { pm.expect(pm.globals.get('k')).to.eql('G'); pm.expect(pm.environment.get('k')).to.eql('E'); });\n",
    );
    write(&dir, "requests/second.yaml", "name: Second\nmethod: GET\nurl: ${base}/second?x=${x}&y=${y}\nscripts:\n  post: assert.status(200);\n");
    let first = server.mock(|when, then| {
        when.method(GET)
            .path("/first")
            .query_param("who", "C")
            .query_param("k", "E")
            .query_param("only", "G")
            .query_param("x", "from-script")
            .header("authorization", "Bearer gl0bal-s3cret");
        then.status(200).body("echo gl0bal-s3cret");
    });
    // A global set by a script lasts for the run; the environment's y still wins.
    let second = server.mock(|when, then| {
        when.method(GET).path("/second").query_param("x", "from-script").query_param("y", "E");
        then.status(200);
    });
    let out = home.path().join("reports");
    hitop(&home)
        .args(["run", "./api", "--env", "mock", "--reporter", "cli,json", "--out"])
        .arg(&out)
        .env("HITOP_SECRET_GLOBAL_TOKEN", "gl0bal-s3cret")
        .assert()
        .success()
        .stdout(contains("2/2 requests succeeded").and(contains("gl0bal-s3cret").not()));
    first.assert();
    second.assert();
    assert!(!fs::read_to_string(out.join("report.json")).unwrap().contains("gl0bal-s3cret"), "secret globals are masked");
    // A missing secret global names its env var and the command to set it.
    hitop(&home)
        .args(["run", "./api", "--env", "mock"])
        .assert()
        .code(2)
        .stderr(contains("no value for global secret token").and(contains("HITOP_SECRET_GLOBAL_TOKEN")).and(contains("hitop secret set token --globals")));
}

#[test]
fn globals_commands_send_and_postman_import() {
    let home = TempDir::new().unwrap();
    let server = MockServer::start();
    hitop(&home).args(["globals", "show"]).assert().success().stdout(contains("No globals"));
    hitop(&home).args(["globals", "set", "base", &server.base_url()]).assert().success();
    hitop(&home).args(["globals", "set", "who", "global"]).assert().success();
    hitop(&home).args(["globals", "unset", "who"]).assert().success();
    hitop(&home).args(["globals", "unset", "nope"]).assert().failure().stderr(contains("no global 'nope'"));

    // Postman globals, through `hitop import` and `hitop env import`, land in the globals.
    let pg = home.path().join("g.postman_globals.json");
    fs::write(&pg, r#"{"name": "Workspace Globals", "_postman_variable_scope": "globals", "values": [{"key": "who", "value": "pm"}, {"key": "apiKey", "value": "k-1", "type": "secret"}]}"#).unwrap();
    hitop(&home)
        .arg("import")
        .arg(&pg)
        .assert()
        .success()
        .stdout(contains("Imported 2 globals into globals"))
        .stderr(contains("HITOP_SECRET_GLOBAL_APIKEY"));
    hitop(&home).args(["env", "list"]).assert().success().stdout(contains("Workspace Globals").not());
    hitop(&home)
        .args(["globals", "show"])
        .assert()
        .success()
        .stdout(contains("base = ").and(contains("who = pm")).and(contains("apiKey = ••••")).and(contains("k-1").not()));
    hitop(&home).args(["globals", "set", "apiKey", "x"]).assert().failure().stderr(contains("hitop secret set apiKey --globals"));
    hitop(&home).args(["secret", "ls", "--globals"]).assert().success().stdout(contains("apiKey"));
    hitop(&home).args(["secret", "ls"]).assert().failure().stderr(contains("--env").or(contains("--globals")));
    let pg2 = home.path().join("g2.json");
    fs::write(&pg2, r#"{"name": "More", "_postman_variable_scope": "globals", "values": [{"key": "who", "value": "pm2"}]}"#).unwrap();
    hitop(&home).args(["env", "import"]).arg(&pg2).assert().success().stdout(contains("Imported 1 global"));

    // `hitop send` without -c uses the stored globals; a secret global from its env var.
    let me = server.mock(|when, then| {
        when.method(GET).path("/me").query_param("who", "pm2").header("x-api-key", "k-1");
        then.status(200).body("ok");
    });
    hitop(&home)
        .args(["send", "--raw", "${base}/me?who=${who}", "-H", "X-API-Key: ${apiKey}"])
        .env("HITOP_SECRET_GLOBAL_APIKEY", "k-1")
        .assert()
        .success()
        .stdout(contains("ok"));
    me.assert();

    // Export: secret names, never values; import of that export keeps them.
    let exported = home.path().join("globals-export.json");
    hitop(&home).args(["globals", "export", "-o"]).arg(&exported).assert().success();
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&exported).unwrap()).unwrap();
    assert_eq!(v, serde_json::json!({ "variables": { "base": server.base_url(), "who": "pm2" }, "secrets": ["apiKey"] }));
    let dir = home.path().join("api");
    hitop(&home).args(["init"]).arg(&dir).assert().success();
    hitop(&home).args(["globals", "import", "-c"]).arg(&dir).arg(&exported).assert().success();
    assert_eq!(fs::read_to_string(dir.join("globals.yaml")).unwrap(), format!("variables:\n  apiKey:\n    secret: true\n  base: {}\n  who: pm2\n", server.base_url()));
    hitop(&home).args(["globals", "show", "-c"]).arg(&dir).assert().success().stdout(contains("who = pm2"));
}

#[test]
fn convert_carries_the_globals() {
    let home = TempDir::new().unwrap();
    let dir = home.path().join("api");
    hitop(&home).args(["init"]).arg(&dir).assert().success();
    write(&dir, "globals.yaml", "variables:\n  g: '1'\n  s:\n    secret: true\n");
    let json = home.path().join("c.json");
    let globals = home.path().join("globals.json");
    hitop(&home).arg("convert").arg(&dir).arg(&json).arg("--globals").arg(&globals).assert().success();
    let g: serde_json::Value = serde_json::from_str(&fs::read_to_string(&globals).unwrap()).unwrap();
    assert_eq!(g["variables"], serde_json::json!({ "g": "1" }));
    assert_eq!(g["secrets"], serde_json::json!(["s"]));
    let back = home.path().join("back");
    hitop(&home).arg("convert").arg(&json).arg(&back).arg("--globals").arg(&globals).assert().success();
    assert_eq!(fs::read_to_string(back.join("globals.yaml")).unwrap(), fs::read_to_string(dir.join("globals.yaml")).unwrap());
}
