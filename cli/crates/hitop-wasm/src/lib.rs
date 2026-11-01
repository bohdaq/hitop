//! hitop-core in the browser: the collection folder format (read and
//! write YAML exactly as the CLI does) and the JSON model, through
//! wasm-bindgen. Every function takes and returns JSON text, so the
//! JavaScript side stays a thin wrapper (`webapp/src/wasm/format.js`).
use hitop_core::format::{files, Folder};
use hitop_core::model::{Collection, Environment, HistoryItem};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// `{collection, environments, globals?}`: a collection folder in memory
/// (`globals`: its globals.yaml; left out, an existing one is kept).
#[derive(Serialize, Deserialize)]
struct FolderJson {
    collection: Collection,
    #[serde(default)]
    environments: Vec<Environment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    globals: Option<Environment>,
}

/// The desktop's `environments.json` (`hitop_core::storage::Environments`,
/// which needs the `native` feature).
#[derive(Serialize, Deserialize)]
struct EnvironmentsJson {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    environments: Vec<Environment>,
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn chain(e: anyhow::Error) -> JsError {
    JsError::new(&format!("{e:#}"))
}

pub fn read_files_json(files_json: &str, name: &str) -> anyhow::Result<String> {
    let files: files::Files = serde_json::from_str(files_json)?;
    let folder = files::read_files(&files, name)?;
    Ok(serde_json::to_string(&FolderJson { collection: folder.collection, environments: folder.environments, globals: folder.globals })?)
}

pub fn to_files_json(folder_json: &str) -> anyhow::Result<String> {
    let f: FolderJson = serde_json::from_str(folder_json)?;
    let files = files::to_files(&Folder { collection: f.collection, environments: f.environments, globals: f.globals })?;
    Ok(serde_json::to_string(&files)?)
}

pub fn plan_sync_json(existing_json: &str, files_json: &str) -> anyhow::Result<String> {
    let existing: Vec<String> = serde_json::from_str(existing_json)?;
    let files: files::Files = serde_json::from_str(files_json)?;
    Ok(serde_json::to_string(&files::plan_sync(&existing, &files)?)?)
}

pub fn normalize_json(kind: &str, json: &str) -> anyhow::Result<String> {
    Ok(match kind {
        "collections" => serde_json::to_string(&serde_json::from_str::<Vec<Collection>>(json)?)?,
        "history" => serde_json::to_string(&serde_json::from_str::<Vec<HistoryItem>>(json)?)?,
        "environments" => serde_json::to_string(&serde_json::from_str::<EnvironmentsJson>(json)?)?,
        "folder" => serde_json::to_string(&serde_json::from_str::<FolderJson>(json)?)?,
        "globals" => {
            let mut v: serde_json::Value = serde_json::from_str(json)?;
            if let Some(o) = v.as_object_mut() {
                o.insert("id".into(), hitop_core::model::GLOBALS_ID.into());
                o.insert("name".into(), hitop_core::model::GLOBALS_ID.into());
            }
            serde_json::to_string(&serde_json::from_value::<Environment>(v)?)?
        }
        other => anyhow::bail!("unknown kind {other}"),
    })
}

/// Reads a collection folder from `{path: text}` JSON; `name` is used when
/// `hitop.yaml` has none. Returns `{collection, environments}`.
#[wasm_bindgen(js_name = readFiles)]
pub fn read_files(files_json: &str, name: &str) -> Result<String, JsError> {
    read_files_json(files_json, name).map_err(chain)
}

/// The files of `{collection, environments}` as `{path: text}` JSON.
#[wasm_bindgen(js_name = toFiles)]
pub fn to_files(folder_json: &str) -> Result<String, JsError> {
    to_files_json(folder_json).map_err(chain)
}

/// What to write and remove in a folder that has `existing` paths so it
/// holds `files`: `{write: {path: text}, remove: [path]}`.
#[wasm_bindgen(js_name = planSync)]
pub fn plan_sync(existing_json: &str, files_json: &str) -> Result<String, JsError> {
    plan_sync_json(existing_json, files_json).map_err(chain)
}

/// Reads JSON into hitop-core's model and writes it back, as the desktop's
/// storage commands do: `collections`, `history`, `environments`
/// (`{active, environments}`) or `folder`. Fails where Rust would.
#[wasm_bindgen]
pub fn normalize(kind: &str, json: &str) -> Result<String, JsError> {
    normalize_json(kind, json).map_err(err)
}

/// A Postman collection through hitop-core's importer, as `{collection,
/// report}`. `scans_json` maps each script's text to its unsupported calls
/// (`[[line, call, text]]`), found by the shim in the page.
pub fn import_postman_json(json: &str, scans_json: &str) -> anyhow::Result<String> {
    let scans: std::collections::HashMap<String, Vec<(usize, String, String)>> = serde_json::from_str(scans_json)?;
    let scan = |code: &str| scans.get(code).cloned().unwrap_or_default();
    let (collection, report) = hitop_core::postman::import_collection_with(json, &scan)?;
    Ok(serde_json::to_string(&serde_json::json!({ "collection": collection, "report": report }))?)
}

/// See [`import_postman_json`].
#[wasm_bindgen(js_name = importPostman)]
pub fn import_postman(json: &str, scans_json: &str) -> Result<String, JsError> {
    import_postman_json(json, scans_json).map_err(chain)
}

/// The JSON body of a GraphQL request whose `${variables}` are filled in
/// (`{"query","variables","operationName"}`), built by the same code as the
/// CLI's (`hitop_core::graphql::request_body`). Fails when the variables
/// aren't a JSON object.
#[wasm_bindgen(js_name = graphqlBody)]
pub fn graphql_body(query: &str, variables: &str, operation_name: &str) -> Result<String, JsError> {
    hitop_core::graphql::request_body(query, variables, operation_name).map_err(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_round_trip_through_json() {
        let folder = r#"{"collection":{"id":1,"name":"Shop","requests":[{"id":2,"name":"Get user","url":"https://x/u","method":"GET","headers":[],"body":"","pinned":true}],"variables":{"a":"1"}},
            "environments":[{"id":"dev","name":"dev","variables":{"t":"s3cr3t"},"secrets":["t"]}]}"#;
        let files = to_files_json(folder).unwrap();
        assert!(files.contains("requests/get-user.yaml") && !files.contains("s3cr3t"), "{files}");
        let back: serde_json::Value = serde_json::from_str(&read_files_json(&files, "x").unwrap()).unwrap();
        assert_eq!(back["collection"]["requests"][0]["pinned"], true);
        assert_eq!(back["environments"][0]["secrets"][0], "t");
        let plan: serde_json::Value = serde_json::from_str(&plan_sync_json(r#"["requests/old.yaml"]"#, &files).unwrap()).unwrap();
        assert_eq!(plan["remove"][0], "requests/old.yaml");
    }

    #[test]
    fn postman_import_takes_the_page_s_script_checks() {
        let c = r#"{"info":{"name":"P"},"item":[{"name":"a","request":{"url":"{{b}}/x","method":"GET"},
            "event":[{"listen":"test","script":{"exec":["pm.sendRequest('x');"]}}]}]}"#;
        let scans = r#"{"pm.sendRequest('x');":[[1,"pm.sendRequest","pm.sendRequest('x');"]]}"#;
        let out: serde_json::Value = serde_json::from_str(&import_postman_json(c, scans).unwrap()).unwrap();
        assert_eq!(out["collection"]["requests"][0]["url"], "${b}/x");
        assert_eq!(out["report"]["issues"][0]["what"], "pm.sendRequest");
        assert_eq!(out["report"]["issues"][0]["location"], "a (post-request script)");
    }

    #[test]
    fn graphql_requests_round_trip() {
        let folder = r#"{"collection":{"id":1,"name":"G","requests":[{"id":2,"name":"Me","url":"https://x/graphql","method":"POST","headers":[],"body":"{ me }","bodyType":"graphql","graphql":{"variables":"{\"a\": 1}","operationName":"Me"}}]}}"#;
        let files = to_files_json(folder).unwrap();
        assert!(files.contains("operationName: Me"), "{files}");
        let back: serde_json::Value = serde_json::from_str(&read_files_json(&files, "x").unwrap()).unwrap();
        assert_eq!(back["collection"]["requests"][0]["graphql"], serde_json::json!({"variables": "{\"a\": 1}", "operationName": "Me"}));
        assert_eq!(hitop_core::graphql::request_body("{ me }", "{\"a\": 1}", "Me").unwrap(), r#"{"query":"{ me }","variables":{"a": 1},"operationName":"Me"}"#);
    }

    #[test]
    fn normalize_reads_what_rust_reads() {
        let out = normalize_json("collections", r#"[{"id":"col-legacy","name":"c","requests":[]}]"#).unwrap();
        assert!(out.starts_with(r#"[{"id":"#) && !out.contains("col-legacy"), "{out}");
        assert!(normalize_json("history", r#"[{"id":1,"timestamp":"2026-10-31T18:16:10.537Z"}]"#).is_err(), "web history items need mapping first");
        assert!(normalize_json("environments", r#"{"environments":[{"id":"a","name":"A"}]}"#).is_ok());
    }
}
