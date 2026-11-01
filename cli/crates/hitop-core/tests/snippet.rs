//! Code snippets: golden files per case and language
//! (`tests/golden/snippets/<case>/<lang>.<ext>`, cases in `cases.json`), and a
//! syntax check of every golden file with the language's own tool where it is
//! installed (bash, node, python3, javac, gofmt; skipped otherwise).
//!
//! Regenerate after an intended change with
//! `UPDATE_GOLDEN=1 cargo test -p hitop-core --test snippet`, then review the diff.
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use hitop_core::model::{Auth, Request};
use hitop_core::snippet::{generate, Lang};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    #[serde(default)]
    variables: HashMap<String, String>,
    #[serde(default)]
    secrets: Vec<String>,
    #[serde(default)]
    collection_auth: Option<Auth>,
    request: Request,
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/snippets")
}

fn cases() -> Vec<Case> {
    serde_json::from_str(&fs::read_to_string(dir().join("cases.json")).unwrap()).unwrap()
}

fn golden(case: &str, lang: Lang) -> PathBuf {
    dir().join(case).join(format!("{}.{}", lang.id(), lang.extension()))
}

#[test]
fn snippets_match_the_golden_files() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    for case in cases() {
        for lang in Lang::ALL {
            let s = generate(lang, &case.request, case.collection_auth.as_ref(), &case.variables, &case.secrets).unwrap();
            for secret in &case.secrets {
                let value = &case.variables[secret];
                assert!(!s.code.contains(value.as_str()), "{} {}: secret {secret} leaked", case.name, lang.id());
            }
            let path = golden(&case.name, lang);
            if update {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, &s.code).unwrap();
            }
            let want = fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {} (UPDATE_GOLDEN=1)", path.display()));
            assert_eq!(s.code, want, "{} differs from the golden file", path.display());
        }
    }
}

#[test]
fn placeholders_are_reported() {
    let case = cases().into_iter().find(|c| c.name == "secrets-placeholders").unwrap();
    let s = generate(Lang::Curl, &case.request, None, &case.variables, &case.secrets).unwrap();
    assert_eq!(s.placeholders, vec!["tenant".to_string(), "token".to_string()]);
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok_and(|o| o.status.success())
        || Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success())
}

fn check(cmd: &mut Command, what: &Path) {
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{} doesn't parse:\n{}{}", what.display(), String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

#[test]
fn golden_files_parse_in_their_language() {
    let tmp = tempfile::tempdir().unwrap();
    for case in cases() {
        for lang in Lang::ALL {
            let path = golden(&case.name, lang);
            match lang {
                Lang::Curl if have("bash") => check(Command::new("bash").arg("-n").arg(&path), &path),
                Lang::Fetch if have("node") => check(Command::new("node").arg("--check").arg(&path), &path),
                Lang::Python if have("python3") => {
                    let code = "import ast, sys; ast.parse(open(sys.argv[1], encoding='utf-8').read())";
                    check(Command::new("python3").args(["-c", code]).arg(&path), &path)
                }
                Lang::Go if have("gofmt") => check(Command::new("gofmt").arg("-e").arg("-l").arg(&path), &path),
                Lang::Java if have("javac") => {
                    // javac wants the file named after its public class.
                    let out = tmp.path().join(&case.name);
                    fs::create_dir_all(&out).unwrap();
                    fs::copy(&path, out.join("Main.java")).unwrap();
                    check(Command::new("javac").arg("-d").arg(&out).arg(out.join("Main.java")), &path)
                }
                _ => {}
            }
        }
    }
}
