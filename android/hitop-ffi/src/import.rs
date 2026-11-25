//! `import` (Postman collections, environments and globals; OpenAPI; HITOP JSON), `openapiSync` and
//! `export`: hitop-core's importers and exporters, as the CLI and the web app use them.

use hitop_core::model::Collection;
use hitop_core::{convert, export as core_export, openapi, postman};
use serde_json::{json, Value};

use crate::Args;

fn text<T: serde::Serialize>(v: &T) -> Result<String, String> {
    // As text: exact 64-bit ids.
    serde_json::to_string(v).map_err(|e| e.to_string())
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn options(a: &Args) -> Result<openapi::Options, String> {
    Ok(openapi::Options { flat: false, source: a.opt_json("source")?, inline_auth: false })
}

pub(crate) fn import(a: &Args) -> Result<Value, String> {
    let t = a.str("text")?.trim_start_matches('\u{feff}');
    if t.trim().is_empty() {
        return Err("The file is empty.".into());
    }
    if openapi::detect(t) {
        let (c, r) = openapi::import(t, &options(a)?).map_err(err)?;
        let issues: Vec<String> = r.skipped.iter().map(|i| format!("Not imported: {i}")).chain(r.warnings.iter().map(|i| format!("Check: {i}"))).collect();
        return Ok(json!({ "kind": "openapi", "collections": [text(&c)?], "issues": issues }));
    }
    if let Some(c) = convert::convert_text(t).map_err(err)? {
        return converted(c);
    }
    let v: Value = serde_json::from_str(t).map_err(|e| format!("This isn't a Postman, Insomnia, Bruno, HAR, OpenAPI or HITOP file ({e})."))?;
    match postman::detect(&v) {
        Some(postman::Kind::Collection) => {
            let (c, r) = postman::import_collection(t).map_err(err)?;
            Ok(json!({ "kind": "postman", "collections": [text(&c)?], "issues": r.issues.iter().map(|i| i.to_string()).collect::<Vec<_>>() }))
        }
        Some(kind) => {
            let e = postman::import_environment(t).map_err(err)?;
            let secrets: std::collections::HashMap<_, _> = e.secret_values.into_iter().collect();
            Ok(json!({ "kind": if kind == postman::Kind::Globals { "globals" } else { "environment" }, "environment": e.environment, "secretValues": secrets, "issues": [] }))
        }
        None => {
            let list = core_export::from_hitop(t).map_err(|_| "This isn't a Postman, OpenAPI or HITOP file.".to_string())?;
            let list = list.iter().map(text).collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "kind": "hitop", "collections": list, "issues": [] }))
        }
    }
}

/// Insomnia, Bruno and HAR: several collections and environments at once.
fn converted(c: convert::Converted) -> Result<Value, String> {
    let mut issues: Vec<String> = c.issues.iter().map(|i| i.to_string()).collect();
    let mut collections = vec![];
    for v in &c.collections {
        let (col, r) = postman::import_collection(&v.to_string()).map_err(err)?;
        collections.push(text(&col)?);
        issues.extend(r.issues.iter().map(|i| i.to_string()));
    }
    let mut environments = vec![];
    for v in &c.environments {
        let e = postman::import_environment(&v.to_string()).map_err(err)?;
        let secrets: std::collections::HashMap<_, _> = e.secret_values.into_iter().collect();
        environments.push(json!({ "environment": e.environment, "secretValues": secrets }));
    }
    Ok(json!({ "kind": c.format, "collections": collections, "environments": environments, "issues": issues }))
}

/// `importFiles {files: {path: text}}`: a Bruno collection folder.
pub(crate) fn import_files(a: &Args) -> Result<Value, String> {
    let files = a.opt_json("files")?.unwrap_or_default();
    let c = convert::convert_files(&files).map_err(err)?.ok_or("This isn't a Bruno collection (no bruno.json).")?;
    converted(c)
}

pub(crate) fn openapi_sync(a: &Args) -> Result<Value, String> {
    let c: Collection = a.text_json("collection")?;
    let prune = a.0.get("prune").and_then(Value::as_bool).unwrap_or(false);
    let s = openapi::sync(&c, a.str("text")?, &options(a)?, prune).map_err(err)?;
    Ok(json!({ "collection": text(&s.collection)?, "diff": s.diff, "empty": s.diff.is_empty() }))
}

pub(crate) fn export(a: &Args) -> Result<Value, String> {
    let c: Collection = a.text_json("collection")?;
    Ok(match a.str("format")? {
        "hitop" => core_export::to_hitop(std::slice::from_ref(&c)),
        "postman" => core_export::to_postman(&c),
        other => return Err(format!("format {other}: hitop or postman")),
    }
    .map_err(err)?
    .into())
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::{json, Value};

    fn call(op: &str, args: Value) -> Value {
        serde_json::from_str(&call_json(op, &args.to_string())).unwrap()
    }
    fn fixture(p: &str) -> String {
        std::fs::read_to_string(format!("{}/../../cli/fixtures/{p}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn the_vendored_files_import_as_the_core_does() {
        let t = fixture("launch-gate/shop.postman_collection.json");
        let out = call("import", json!({ "text": t }));
        let (c, _) = hitop_core::postman::import_collection(&t).unwrap();
        let got: hitop_core::model::Collection = serde_json::from_str(out["ok"]["collections"][0].as_str().unwrap()).unwrap();
        assert_eq!(got.requests.len(), c.requests.len());
        let exported = call("export", json!({ "collection": out["ok"]["collections"][0], "format": "hitop" }));
        let back = call("import", json!({ "text": exported["ok"] }));
        assert_eq!(back["ok"]["kind"], "hitop");
        assert_eq!(serde_json::from_str::<Value>(back["ok"]["collections"][0].as_str().unwrap()).unwrap(), serde_json::to_value(&got).unwrap());
        let pm = call("export", json!({ "collection": out["ok"]["collections"][0], "format": "postman" }));
        assert_eq!(call("import", json!({ "text": pm["ok"] }))["ok"]["kind"], "postman");
    }

    #[test]
    fn secrets_of_a_postman_environment_come_apart() {
        let t = r#"{"name":"S","values":[{"key":"a","value":"1"},{"key":"t","value":"s3","type":"secret"}]}"#;
        let out = call("import", json!({ "text": t }));
        assert_eq!(out["ok"]["kind"], "environment");
        assert_eq!(out["ok"]["secretValues"]["t"], "s3");
        assert!(!out["ok"]["environment"].to_string().contains("s3"));
    }

    #[test]
    fn a_spec_imports_and_syncs() {
        let spec = "openapi: 3.0.0\ninfo: {title: P}\nservers: [{url: /v1}]\npaths:\n  /a: {get: {operationId: a, responses: {}}}\n";
        let out = call("import", json!({ "text": spec, "source": "https://x.test/spec.yaml" }));
        assert_eq!(out["ok"]["kind"], "openapi");
        let next = spec.replace("/a: {get: {operationId: a", "/b: {get: {operationId: b");
        let s = call("openapiSync", json!({ "collection": out["ok"]["collections"][0], "text": next }));
        assert_eq!(s["ok"]["diff"]["added"][0]["operationId"], "b");
        assert!(call("import", json!({ "text": "hello" }))["error"].as_str().unwrap().contains("isn't"));
        assert!(call("import", json!({ "text": "[1]" })).get("error").is_some());
    }

    #[test]
    fn insomnia_har_and_a_bruno_folder() {
        let out = call("import", json!({ "text": fixture("imports/insomnia/environments.yaml") }));
        assert_eq!(out["ok"]["kind"], "insomnia", "{out}");
        assert_eq!(out["ok"]["environments"][0]["environment"]["name"], "ExampleA");
        let out = call("import", json!({ "text": fixture("imports/har/recording.har") }));
        assert!(out["ok"]["kind"] == "har" && !out.to_string().contains("recorded-s3cret"), "{out}");
        let files = json!({ "x/bruno.json": fixture("imports/bruno/testbench/bruno.json"), "x/ping.bru": fixture("imports/bruno/testbench/ping.bru") });
        let out = call("importFiles", json!({ "files": files }));
        assert!(out["ok"]["collections"][0].as_str().unwrap().contains("${host}/ping"), "{out}");
    }
}
