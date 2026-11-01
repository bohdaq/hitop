use super::*;
use crate::model::{Auth, FormField, Header, Param, Request};

fn req(name: &str, url: &str) -> Request {
    Request { id: 7, name: name.into(), url: url.into(), method: "GET".into(), ..Default::default() }
}

fn col(requests: Vec<Request>) -> Collection {
    Collection { id: 1, name: "c".into(), requests, ..Default::default() }
}

fn round_trip(c: &Collection) -> Collection {
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), c).unwrap();
    read_folder(dir.path()).unwrap().collection
}

fn file(dir: &Path, rel: &str) -> String {
    fs::read_to_string(dir.join(rel)).unwrap()
}

#[test]
fn query_goes_to_params_only_when_the_url_rebuilds_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = req("A", "${base}/s?q=1&flag#top");
    a.disabled_params = vec![Param { key: "debug".into(), value: "1".into(), ..Default::default() }];
    let odd = ["https://x/?", "https://x/?a=1&&b", "https://x/#f?g", "https://x/?a=1#f?g", "https://x/?k=a b"];
    let mut requests = vec![a];
    requests.extend(odd.iter().enumerate().map(|(i, u)| req(&format!("odd {i}"), u)));
    let c = col(requests);
    write_collection(dir.path(), &c).unwrap();
    let a_yaml = file(dir.path(), "requests/a.yaml");
    assert!(a_yaml.contains("url: ${base}/s#top\n"), "{a_yaml}");
    assert!(a_yaml.contains("- name: debug\n  value: '1'\n  enabled: false\n"), "{a_yaml}");
    for i in 0..odd.len() {
        assert!(!file(dir.path(), &format!("requests/odd-{i}.yaml")).contains("params:"));
    }
    assert_eq!(read_folder(dir.path()).unwrap().collection, c);
}

#[test]
fn body_types() {
    let json = |body: &str, ct: Option<&str>| {
        let mut r = req("r", "u");
        r.body = body.into();
        r.headers = ct.map(|c| vec![Header::new("Content-Type", c)]).unwrap_or_default();
        infer_body_type(&r.headers, &r.body)
    };
    assert_eq!(json("", None), "none");
    assert_eq!(json("{\"a\":1}", None), "json");
    assert_eq!(json("{not json", None), "text");
    assert_eq!(json("<a/>", None), "xml");
    assert_eq!(json("x", Some("application/vnd.api+json")), "json");
    assert_eq!(json("{}", Some("text/plain")), "text");

    let mut gql = req("q", "u");
    gql.body = "query { me { id } }".into();
    gql.body_type = Some("graphql".into());
    let mut form = req("f", "u");
    form.body_mode = Some("urlencoded".into());
    form.body = "left over raw text".into();
    form.form_fields = vec![FormField { key: "a".into(), value: "1".into(), ..Default::default() }];
    let mut multi = req("m", "u");
    multi.body_mode = Some("multipart".into());
    multi.form_fields = vec![FormField { key: "file".into(), kind: "file".into(), enabled: false, ..Default::default() }];
    let c = col(vec![gql, form, multi]);
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), &c).unwrap();
    assert!(file(dir.path(), "requests/q.yaml").contains("body:\n  type: graphql\n"));
    assert!(file(dir.path(), "requests/f.yaml").contains("type: form\n"));
    assert!(file(dir.path(), "requests/m.yaml").contains("type: multipart\n"));
    assert_eq!(read_folder(dir.path()).unwrap().collection, c);

    let mut bad = req("b", "u");
    bad.body_mode = Some("json".into());
    assert!(write_collection(dir.path(), &col(vec![bad])).is_err());
}

#[test]
fn multiline_scripts_and_bodies_are_block_scalars() {
    let mut r = req("r", "u");
    r.body = "{\n  \"a\": 1\n}\n".into();
    r.post_request_script = "assert.status(200);\nsetContext('id', 1);".into();
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), &col(vec![r])).unwrap();
    let y = file(dir.path(), "requests/r.yaml");
    assert!(y.contains("  content: |\n    {\n      \"a\": 1\n    }\n"), "{y}");
    assert!(y.contains("scripts:\n  post: |-\n    assert.status(200);\n"), "{y}");
}

#[test]
fn blank_header_rows_are_not_written() {
    let mut r = req("r", "u");
    r.headers = vec![Header::new("", ""), Header::new("A", "1")];
    let back = round_trip(&col(vec![r]));
    assert_eq!(back.requests[0].headers, vec![Header::new("A", "1")]);
}

#[test]
fn unknown_keys_survive_a_save() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("requests/orders")).unwrap();
    fs::write(dir.path().join(MANIFEST), "format: 1\nname: shop\nowner: team-a\n").unwrap();
    fs::write(
        dir.path().join("requests/orders/create.yml"),
        "name: Create\nmethod: POST\nurl: https://x/orders\nheaders:\n- name: A\n  value: 1\n  note: kept\ntags: [smoke]\n",
    )
    .unwrap();
    let mut folder = read_folder(dir.path()).unwrap();
    let r = &mut folder.collection.requests[0];
    assert_eq!(r.file.as_deref(), Some("orders/create"));
    assert_eq!(r.headers[0].value, "1");
    r.url = "https://x/v2/orders".into();
    write_collection(dir.path(), &folder.collection).unwrap();
    let y = file(dir.path(), "requests/orders/create.yml");
    assert!(y.contains("url: https://x/v2/orders\n") && y.contains("tags:\n- smoke\n") && y.contains("  note: kept\n"), "{y}");
    assert!(!dir.path().join("requests/orders/create.yaml").exists());
    assert!(file(dir.path(), MANIFEST).contains("owner: team-a\n"));
}

#[test]
fn ids_come_from_paths_and_stay_stable() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("requests")).unwrap();
    fs::write(dir.path().join(MANIFEST), "format: 1\nname: x\n").unwrap();
    fs::write(dir.path().join("requests/ping.yaml"), "url: https://x/ping\n").unwrap();
    let a = read_folder(dir.path()).unwrap();
    let b = read_folder(dir.path()).unwrap();
    assert_eq!(a, b);
    let r = &a.collection.requests[0];
    assert_eq!((r.name.as_str(), r.method.as_str(), r.id), ("ping", "GET", names::request_id("ping")));
    write_collection(dir.path(), &a.collection).unwrap();
    assert!(!file(dir.path(), "requests/ping.yaml").contains("id:"));
}

#[test]
fn order_is_kept_and_deleted_requests_lose_their_file() {
    let mut c = col(vec![req("Zed", "u"), req("Alpha", "u"), req("Mid", "u")]);
    c.requests[2].file = Some("nested/deep/mid".into());
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), &c).unwrap();
    assert!(file(dir.path(), MANIFEST).contains("order:\n- zed\n- alpha\n- nested/deep/mid\n"));
    assert_eq!(read_folder(dir.path()).unwrap().collection, c);
    c.requests.remove(2);
    write_collection(dir.path(), &c).unwrap();
    assert!(!dir.path().join("requests/nested").exists());
    c.requests.reverse();
    write_collection(dir.path(), &c).unwrap();
    assert!(!file(dir.path(), MANIFEST).contains("order:"), "sorted order is the default");
}

#[test]
fn paths_that_leave_the_folder_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(MANIFEST), "format: 1\nname: x\norder: [../../etc/passwd]\n").unwrap();
    assert!(read_folder(dir.path()).is_err());
    let mut r = req("r", "u");
    r.file = Some("../escape".into());
    assert!(write_collection(dir.path(), &col(vec![r])).is_err());
}

#[test]
fn format_version_is_checked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(MANIFEST), "name: x\n").unwrap();
    assert!(read_folder(dir.path()).unwrap_err().to_string().contains("format"));
    fs::write(dir.path().join(MANIFEST), "format: 2\nname: x\n").unwrap();
    assert!(format!("{:#}", read_folder(dir.path()).unwrap_err()).contains("newer hitop"));
}

#[test]
fn reserved_keys_are_not_carried_over() {
    let mut r = req("r", "u");
    r.extra.insert("scripts".into(), serde_json::json!("x"));
    assert!(write_collection(tempfile::tempdir().unwrap().path(), &col(vec![r])).is_err());
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(MANIFEST), "format: 1\nname: x\nrequests: []\n").unwrap();
    assert!(read_folder(dir.path()).is_err());
}

#[test]
fn secrets_have_no_value_in_the_file() {
    let env = Environment {
        id: "prod".into(),
        name: "prod".into(),
        variables: [("base".to_string(), "https://api".to_string()), ("token".to_string(), "s3cr3t-value".to_string())].into(),
        secrets: vec!["token".into()],
        ..Default::default()
    };
    let dir = tempfile::tempdir().unwrap();
    write_folder(dir.path(), &Folder { collection: col(vec![]), environments: vec![env] }).unwrap();
    let y = file(dir.path(), "environments/prod.yaml");
    assert_eq!(y, "variables:\n  base: https://api\n  token:\n    secret: true\n");
    let back = &read_folder(dir.path()).unwrap().environments[0];
    assert_eq!(back.secrets, ["token"]);
    assert!(!back.variables.contains_key("token"));
}

#[test]
fn scalar_values_read_as_strings() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("environments")).unwrap();
    fs::write(dir.path().join(MANIFEST), "format: 1\nname: x\nvariables:\n  port: 8080\n  debug: true\n").unwrap();
    fs::write(dir.path().join("environments/dev.yaml"), "variables:\n  retries: 3\n").unwrap();
    let f = read_folder(dir.path()).unwrap();
    assert_eq!(f.collection.variables["port"], "8080");
    assert_eq!(f.collection.variables["debug"], "true");
    assert_eq!(f.environments[0].variables["retries"], "3");
    assert_eq!(f.environments[0].name, "dev");
}

#[test]
fn collection_auth_round_trips() {
    let mut c = col(vec![req("r", "u")]);
    c.auth = Some(Auth::Bearer { token: "${token}".into() });
    c.requests[0].auth = Some(Auth::None);
    assert_eq!(round_trip(&c), c);
}

#[test]
fn init_writes_a_starter_and_refuses_to_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("my-api");
    let made = init(&target, None).unwrap();
    assert_eq!(made.collection.name, "my-api");
    let ws = made.collection.workspace.clone().unwrap();
    assert_eq!(ws.len(), 32);
    assert!(file(&target, "hitop.yaml").contains(&format!("workspace: {ws}\n")));
    assert_eq!(read_folder(&target).unwrap(), made);
    assert!(init(&target, None).is_err());
}

#[test]
fn workspace_round_trips_and_is_written_after_id() {
    let mut c = col(vec![]);
    c.workspace = Some("0123abcd".into());
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), &c).unwrap();
    assert_eq!(file(dir.path(), "hitop.yaml"), "format: 1\nname: c\nid: 1\nworkspace: 0123abcd\n");
    assert_eq!(read_folder(dir.path()).unwrap().collection, c);
}

#[test]
fn mark_secret_edits_only_that_environment_file() {
    let dir = tempfile::tempdir().unwrap();
    write_collection(dir.path(), &col(vec![])).unwrap();
    let envs = dir.path().join("environments");
    std::fs::create_dir_all(&envs).unwrap();
    std::fs::write(envs.join("Prod.yml"), "name: prod\nvariables:\n  base: https://api\n  token: plain\n").unwrap();
    std::fs::write(envs.join("dev.yaml"), "variables:\n  token: dev-token\n").unwrap();
    assert!(mark_secret(dir.path(), "prod", "token").unwrap());
    assert!(!mark_secret(dir.path(), "prod", "token").unwrap());
    assert_eq!(file(dir.path(), "environments/Prod.yml"), "name: prod\nvariables:\n  base: https://api\n  token:\n    secret: true\n");
    assert_eq!(file(dir.path(), "environments/dev.yaml"), "variables:\n  token: dev-token\n");
    assert!(mark_secret(dir.path(), "staging", "token").is_err());
}
