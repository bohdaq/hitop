//! spec/vectors/send.json: the Content-Type and scheme rules every surface
//! applies when sending (the web app and HitopKit test the same file).
use std::path::PathBuf;

use hitop_core::sending::{add_content_type, raw_content_type, with_scheme};
use serde_json::Value;

fn vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spec/vectors/send.json");
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn pairs(v: &Value) -> Vec<(String, String)> {
    v.as_array().unwrap().iter().map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string())).collect()
}

#[test]
fn content_type_detect() {
    let v = vectors();
    let cases = v["contentType"]["detect"].as_array().unwrap();
    assert!(cases.len() > 20);
    for c in cases {
        let body = c["body"].as_str().unwrap();
        assert_eq!(raw_content_type(body), c["contentType"].as_str(), "body {body:?}");
    }
}

#[test]
fn content_type_apply() {
    let v = vectors();
    for c in v["contentType"]["apply"].as_array().unwrap() {
        let mut headers = pairs(&c["headers"]);
        add_content_type(c["method"].as_str().unwrap(), &mut headers, c["body"].as_str().unwrap());
        assert_eq!(headers, pairs(&c["sent"]), "{c}");
    }
}

#[test]
fn scheme() {
    let v = vectors();
    for c in v["scheme"]["cases"].as_array().unwrap() {
        let url = c["url"].as_str().unwrap();
        match (with_scheme(url), c.get("sent")) {
            (Ok(got), Some(want)) => assert_eq!(got, want.as_str().unwrap(), "url {url:?}"),
            (Err(e), None) => assert_eq!(e.to_string(), c["error"].as_str().unwrap(), "url {url:?}"),
            (got, _) => panic!("url {url:?}: got {got:?}, want {c}"),
        }
    }
}
