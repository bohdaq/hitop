//! The format v1 compatibility suite: `spec/compat/v1/<case>/`.
//!
//! Every case is a collection folder (`folder/`) written by some v1 tool and
//! `expected.json`, the JSON model it reads as (`{collection, environments,
//! globals}`). Format 1 is stable: these files never change, and every
//! future reader must read every case to exactly its `expected.json`.
//! Within v1 only new cases are added. A new case has no `expected.json` yet:
//! the first run writes it and fails, so it is reviewed before it is committed.
//!
//! For each case the test also writes the folder back and reads it again
//! (same model), reads it through the in-memory reader the web app uses
//! (same model), and, unless `case.json` says `"canonical": false`, checks
//! that the writer reproduces the case's files byte for byte.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hitop_core::format::{self, files, Folder};
use serde_json::{json, Value};

fn compat_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../spec/compat/v1")
}

fn cases() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(compat_dir())
        .expect("spec/compat/v1")
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("folder").is_dir())
        .collect();
    out.sort();
    out
}

/// The YAML files under `dir`, by `/`-separated path (what a reader looks at).
fn tree(dir: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn model(f: &Folder) -> Value {
    json!({ "collection": f.collection, "environments": f.environments, "globals": f.globals })
}

/// The one normalization a write makes on purpose (spec section 6): blank
/// header rows (empty name and value) aren't written.
fn without_blank_headers(mut v: Value) -> Value {
    for r in v["collection"]["requests"].as_array_mut().into_iter().flatten() {
        if let Some(rows) = r.get_mut("headers").and_then(Value::as_array_mut) {
            rows.retain(|h| !(h["name"] == "" && h["value"] == "" && h.as_object().is_some_and(|o| o.keys().all(|k| k == "name" || k == "value" || k == "enabled"))));
        }
    }
    v
}

fn canonical(case: &Path) -> bool {
    match fs::read_to_string(case.join("case.json")) {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap()["canonical"] != json!(false),
        Err(_) => true,
    }
}

fn check(case: &Path) {
    let name = case.file_name().unwrap().to_string_lossy().to_string();
    let dir = case.join("folder");
    let read = format::read_folder(&dir).unwrap_or_else(|e| panic!("{name}: a v1 reader must read it: {e:#}"));
    let got = model(&read);

    let expected_path = case.join("expected.json");
    let Ok(expected_text) = fs::read_to_string(&expected_path) else {
        fs::write(&expected_path, serde_json::to_string_pretty(&got).unwrap() + "\n").unwrap();
        panic!("{name}: wrote {} for a new case; review it, then commit it", expected_path.display());
    };
    let expected: Value = serde_json::from_str(&expected_text).unwrap();
    assert_eq!(got, expected, "{name}: the model read from folder/ differs from expected.json");

    // The in-memory reader (web app, zip files) gives the same model. Like
    // read_folder it sees only YAML files; hidden ones are its own rule.
    let mut mem = files::Files::new();
    for (path, text) in tree(&dir) {
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            mem.insert(path, text);
        }
    }
    let from_files = files::read_files(&mem, "folder").unwrap_or_else(|e| panic!("{name}: in-memory read: {e:#}"));
    assert_eq!(model(&from_files), expected, "{name}: read_files differs from read_folder");

    // Write it back and read it again: nothing lost.
    let out = tempfile::tempdir().unwrap();
    let written = out.path().join("folder");
    format::write_folder(&written, &read).unwrap();
    let again = format::read_folder(&written).unwrap();
    assert_eq!(model(&again), without_blank_headers(expected.clone()), "{name}: write then read changed the model");

    if canonical(case) {
        let (want, have) = (tree(&dir), tree(&written));
        assert_eq!(have.keys().collect::<Vec<_>>(), want.keys().collect::<Vec<_>>(), "{name}: the writer wrote other files");
        for (path, text) in &want {
            assert_eq!(&have[path], text, "{name}: the writer changed {path}");
        }
    }
}

#[test]
fn every_v1_case_reads_round_trips_and_writes_back() {
    let cases = cases();
    let names: Vec<String> = cases.iter().map(|c| c.file_name().unwrap().to_string_lossy().to_string()).collect();
    // Removing a case breaks the promise; this list only grows.
    for required in ["examples", "graphql", "grpc", "http", "minimal", "openapi", "openapi-rows", "quirks", "secrets", "unknown-keys", "websocket"] {
        assert!(names.iter().any(|n| n == required), "spec/compat/v1/{required} is missing");
    }
    for case in &cases {
        check(case);
    }
}

#[test]
fn a_later_format_version_is_refused_not_guessed() {
    let mut mem = files::Files::new();
    mem.insert("hitop.yaml".into(), "format: 2\nname: Later\n".into());
    let err = files::read_files(&mem, "x").unwrap_err();
    assert!(format!("{err:#}").contains("format 2 needs a newer hitop"), "{err:#}");
    mem.insert("hitop.yaml".into(), "name: No version\n".into());
    assert!(files::read_files(&mem, "x").is_err());
}

#[cfg(feature = "native")]
/// Every key in `a` is in `b` with a value of the same JSON type (arrays
/// compared element by element), so a v1 reader of `a` can read `b`.
fn same_shape(a: &Value, b: &Value, at: &str) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                let w = b.get(k).unwrap_or_else(|| panic!("{at}.{k} is gone"));
                same_shape(v, w, &format!("{at}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{at}: length");
            for (i, (v, w)) in a.iter().zip(b).enumerate() {
                same_shape(v, w, &format!("{at}[{i}]"));
            }
        }
        (Value::Number(_), Value::Number(_)) | (Value::String(_), Value::String(_)) | (Value::Bool(_), Value::Bool(_)) | (Value::Null, Value::Null) => {}
        _ => panic!("{at}: {a} became {b}"),
    }
}

/// `spec/compat/v1/run-report/report.json` is a version 1 report (spec/run-report.md).
/// Today's report model reads it and writes every one of its keys back with the
/// same type: fields may be added within v1, never removed or retyped.
#[cfg(feature = "native")] // the report model is native-only
#[test]
fn the_v1_run_report_reads_and_keeps_every_field() {
    let text = fs::read_to_string(compat_dir().join("run-report/report.json")).unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fixture["reportVersion"], 1);
    let report: hitop_core::report::Report = serde_json::from_str(&text).expect("a v1 report reads");
    let written = serde_json::to_value(&report).unwrap();
    same_shape(&fixture, &written, "report");
}
