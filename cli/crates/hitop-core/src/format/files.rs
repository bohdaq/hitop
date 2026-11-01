//! Collection folders in memory, for places without a file system: the web
//! app (through hitop-wasm), `.zip` files and directory handles. A folder
//! is a map from the path inside it (`/`-separated, with the extension:
//! `hitop.yaml`, `requests/orders/create.yaml`, `environments/dev.yaml`,
//! `globals.yaml`)
//! to the file's text. Reading and writing follow the same rules as
//! [`read_folder`](super::read_folder) and [`write_folder`](super::write_folder),
//! and give the same results (tests below compare both on the golden folder).
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};

use super::convert::{self, check_extra, COLLECTION_KEYS};
use super::doc::{CollectionDoc, EnvironmentDoc, RequestDoc, FORMAT_VERSION};
use super::{names, Folder, ENVIRONMENTS, GLOBALS, MANIFEST, REQUESTS};
use crate::model::Collection;

/// Path inside the folder → file text.
pub type Files = BTreeMap<String, String>;

fn parse<T: serde::de::DeserializeOwned>(path: &str, text: &str) -> Result<T> {
    yaml_serde::from_str(text).with_context(|| path.to_string())
}

/// The YAML files directly in `sub` (or anywhere under it, when
/// `recursive`), by path under `sub` without the extension. Hidden
/// entries are skipped; `a.yaml` next to `a.yml` is an error.
fn yaml_files<'a>(files: &'a Files, sub: &str, recursive: bool) -> Result<BTreeMap<String, (&'a str, &'a str)>> {
    let mut out: BTreeMap<String, (&str, &str)> = BTreeMap::new();
    let prefix = format!("{sub}/");
    for (path, text) in files {
        let Some(rest) = path.strip_prefix(&prefix) else { continue };
        if rest.split('/').any(|seg| seg.starts_with('.') || seg.is_empty()) || (!recursive && rest.contains('/')) {
            continue;
        }
        let Some(stem) = rest.strip_suffix(".yaml").or_else(|| rest.strip_suffix(".yml")) else { continue };
        if let Some((prev, _)) = out.insert(stem.to_string(), (path.as_str(), text.as_str())) {
            bail!("{prev} and {path} are the same request; keep one");
        }
    }
    Ok(out)
}

/// Reads a collection folder from its files. `name` is the collection's
/// name when `hitop.yaml` has none (the folder's name).
pub fn read_files(files: &Files, name: &str) -> Result<Folder> {
    let Some(manifest_text) = files.get(MANIFEST) else {
        bail!("not a collection folder: there is no {MANIFEST}");
    };
    let manifest: CollectionDoc = parse(MANIFEST, manifest_text)?;
    match manifest.format {
        None => bail!("{MANIFEST}: `format: {FORMAT_VERSION}` is missing"),
        Some(v) if v > FORMAT_VERSION => bail!("{MANIFEST}: format {v} needs a newer hitop"),
        Some(_) => {}
    }
    check_extra(&manifest.extra, COLLECTION_KEYS, MANIFEST)?;

    let requests_by_path = yaml_files(files, REQUESTS, true)?;
    let mut ordered: Vec<String> = vec![];
    let mut seen = BTreeSet::new();
    for entry in &manifest.order {
        names::check_rel_path(entry).with_context(|| format!("{MANIFEST}: order"))?;
        if requests_by_path.contains_key(entry) && seen.insert(entry.clone()) {
            ordered.push(entry.clone());
        }
    }
    ordered.extend(requests_by_path.keys().filter(|k| !seen.contains(*k)).cloned());

    let mut requests = Vec::with_capacity(ordered.len());
    for file in &ordered {
        let (path, text) = requests_by_path[file];
        names::check_rel_path(file).with_context(|| path.to_string())?;
        let doc: RequestDoc = parse(path, text)?;
        requests.push(convert::doc_to_request(doc, file)?);
    }
    let pairs: Vec<(&str, &str)> = requests.iter().zip(&ordered).map(|(r, f)| (r.name.as_str(), f.as_str())).collect();
    let explicit = names::explicit_files(&pairs);
    for (r, f) in requests.iter_mut().zip(explicit) {
        r.file = f;
    }

    let name = manifest.name.unwrap_or_else(|| name.to_string());
    let collection = Collection {
        id: manifest.id.unwrap_or_else(|| names::collection_id(&name)),
        name,
        requests,
        workspace: manifest.workspace,
        variables: manifest.variables.into_iter().collect(),
        auth: manifest.auth,
        extra: manifest.extra,
    };

    let mut environments = vec![];
    for (stem, (path, text)) in yaml_files(files, ENVIRONMENTS, false)? {
        let doc: EnvironmentDoc = parse(path, text)?;
        environments.push(convert::doc_to_environment(doc, &stem).with_context(|| path.to_string())?);
    }
    let mut globals = None;
    for path in [GLOBALS, "globals.yml"] {
        if let Some(text) = files.get(path) {
            globals = Some(super::doc_to_globals(parse(path, text)?).with_context(|| path.to_string())?);
            break;
        }
    }
    Ok(Folder { collection, environments, globals })
}

/// Every file of a folder: `hitop.yaml`, `requests/` and `environments/`,
/// with `.yaml` extensions. Secret values are never in them.
pub fn to_files(folder: &Folder) -> Result<Files> {
    let (manifest, requests) = super::collection_files(&folder.collection)?;
    let environments = super::environment_files(&folder.environments)?;
    let mut out = Files::new();
    out.insert(MANIFEST.to_string(), manifest);
    out.extend(requests.into_iter().map(|(k, v)| (format!("{REQUESTS}/{k}.yaml"), v)));
    out.extend(environments.into_iter().map(|(k, v)| (format!("{ENVIRONMENTS}/{k}.yaml"), v)));
    if let Some(g) = &folder.globals {
        out.insert(GLOBALS.to_string(), super::globals_file(g)?);
    }
    Ok(out)
}

/// What to change in an existing folder so it holds `files` (from
/// [`to_files`]), as [`write_folder`](super::write_folder) does on disk: an
/// existing `.yml` file keeps its extension, and YAML files under
/// `requests/` and `environments/` that are no longer wanted are removed.
/// Other files are left alone.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct SyncPlan {
    /// Path → text to write (every wanted file; skip the ones whose text is
    /// already the same).
    pub write: Files,
    /// Paths to remove. Afterwards, remove the folders under `requests/`
    /// that are left empty.
    pub remove: Vec<String>,
}

pub fn plan_sync(existing: &[String], files: &Files) -> Result<SyncPlan> {
    let present: Files = existing.iter().map(|p| (p.clone(), String::new())).collect();
    let mut plan = SyncPlan::default();
    let mut placed = BTreeSet::new();
    for (sub, recursive) in [(REQUESTS, true), (ENVIRONMENTS, false)] {
        let have = yaml_files(&present, sub, recursive)?;
        let want = yaml_files(files, sub, recursive)?;
        for (stem, (path, text)) in &want {
            let target = have.get(stem).map(|(p, _)| p.to_string()).unwrap_or_else(|| path.to_string());
            placed.insert(path.to_string());
            plan.write.insert(target, text.to_string());
        }
        plan.remove.extend(have.iter().filter(|(stem, _)| !want.contains_key(*stem)).map(|(_, (p, _))| p.to_string()));
    }
    for (path, text) in files {
        if !placed.contains(path) {
            // An existing globals.yml keeps its name.
            let target = if path == GLOBALS && !existing.iter().any(|p| p == GLOBALS) && existing.iter().any(|p| p == "globals.yml") { "globals.yml".to_string() } else { path.clone() };
            plan.write.insert(target, text.clone());
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Environment, Header, Request};
    use std::fs;
    use std::path::Path;

    fn tree(base: &Path) -> Files {
        fn walk(base: &Path, dir: &Path, out: &mut Files) {
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
        let mut out = Files::new();
        walk(base, base, &mut out);
        out
    }

    fn golden() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every-field/folder")
    }

    #[test]
    fn reads_the_golden_folder_like_read_folder() {
        let files = tree(&golden());
        let from_disk = super::super::read_folder(&golden()).unwrap();
        assert_eq!(read_files(&files, "folder").unwrap(), from_disk);
        assert_eq!(to_files(&from_disk).unwrap(), files, "writing gives the same files back");
    }

    #[test]
    fn writes_what_write_folder_writes() {
        let folder = Folder {
            collection: Collection {
                id: 7,
                name: "Shop".into(),
                requests: vec![
                    Request { id: 1, name: "Get user".into(), method: "GET".into(), url: "https://x/u?a=1".into(), ..Default::default() },
                    Request { id: 2, name: "Create".into(), method: "POST".into(), url: "https://x/o".into(), file: Some("orders/create".into()), headers: vec![Header::new("Content-Type", "application/json")], body: "{\"a\":1}".into(), ..Default::default() },
                ],
                ..Default::default()
            },
            environments: vec![Environment { id: "dev".into(), name: "dev".into(), variables: [("k".to_string(), "v".to_string()), ("token".to_string(), "s3cr3t".to_string())].into(), secrets: vec!["token".into()], ..Default::default() }],
            globals: Some(Environment { variables: [("g".to_string(), "1".to_string()), ("gk".to_string(), "hidden".to_string())].into(), secrets: vec!["gk".into()], ..Environment::globals() }),
        };
        let dir = tempfile::tempdir().unwrap();
        super::super::write_folder(dir.path(), &folder).unwrap();
        let files = to_files(&folder).unwrap();
        assert_eq!(tree(dir.path()), files);
        assert!(!files.values().any(|t| t.contains("s3cr3t")), "secret values never reach the files");
        let mut back = read_files(&files, "ignored").unwrap();
        assert_eq!(back, super::super::read_folder(dir.path()).unwrap());
        back.environments[0].variables.insert("token".into(), "s3cr3t".into());
        back.globals.as_mut().unwrap().variables.insert("gk".into(), "hidden".into());
        assert_eq!(back, folder);
        assert_eq!(files["globals.yaml"], "variables:\n  g: '1'\n  gk:\n    secret: true\n");
    }

    #[test]
    fn reading_skips_hidden_and_other_files_and_refuses_duplicates() {
        let mut files: Files = [
            ("hitop.yaml", "format: 1\n"),
            ("requests/a.yaml", "url: https://a\n"),
            ("requests/.hidden.yaml", "url: x\n"),
            ("requests/.git/b.yaml", "url: x\n"),
            ("requests/notes.txt", "x"),
            ("environments/dev.yml", "variables:\n  k: v\n"),
            ("environments/nested/x.yaml", "variables: {}\n"),
            ("README.md", "hi"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let f = read_files(&files, "my-api").unwrap();
        assert_eq!(f.collection.name, "my-api");
        assert_eq!(f.collection.requests.len(), 1);
        assert_eq!(f.environments.len(), 1);
        files.insert("requests/a.yml".into(), "url: x\n".into());
        assert!(read_files(&files, "x").unwrap_err().to_string().contains("same request"));
        assert!(read_files(&Files::new(), "x").unwrap_err().to_string().contains("not a collection folder"));
        let newer: Files = [("hitop.yaml".to_string(), "format: 9\n".to_string())].into();
        assert!(read_files(&newer, "x").unwrap_err().to_string().contains("newer hitop"));
    }

    #[test]
    fn websocket_requests() {
        let files = |req: &str| -> Files { [("hitop.yaml".to_string(), "format: 1\n".to_string()), ("requests/ws.yaml".to_string(), req.to_string())].into() };
        let f = read_files(&files("kind: websocket\nurl: wss://x/s\nwebsocket:\n  subprotocols: [chat, 2]\n  messages:\n  - name: hi\n    content: hello ${who}\n  - name: empty\n"), "x").unwrap();
        let r = &f.collection.requests[0];
        assert_eq!(r.kind.as_deref(), Some("websocket"));
        let w = r.websocket.as_ref().unwrap();
        assert_eq!(w.subprotocols, ["chat", "2"]);
        assert_eq!((w.messages[0].name.as_str(), w.messages[0].content.as_str()), ("hi", "hello ${who}"));
        assert_eq!(w.messages[1].content, "");
        // `kind: http` is the default; an empty section is none.
        let plain = read_files(&files("kind: http\nurl: https://x\nwebsocket: {}\n"), "x").unwrap();
        assert_eq!((plain.collection.requests[0].kind.clone(), plain.collection.requests[0].websocket.clone()), (None, None));
        // Unknown keys in the section are an error, as in `body`.
        let err = read_files(&files("kind: websocket\nwebsocket:\n  protocols: [chat]\n"), "x").unwrap_err();
        assert!(format!("{err:#}").contains("protocols"), "{err:#}");
    }

    #[test]
    fn grpc_requests() {
        let files = |req: &str| -> Files { [("hitop.yaml".to_string(), "format: 1\n".to_string()), ("requests/g.yaml".to_string(), req.to_string())].into() };
        let f = read_files(&files("kind: grpc\nurl: localhost:50051\ngrpc:\n  service: a.B\n  method: C\n  message: |\n    {\"n\": ${n}}\n  protos: [p/a.proto]\n  plaintext: true\n"), "x").unwrap();
        let r = &f.collection.requests[0];
        assert_eq!(r.kind.as_deref(), Some("grpc"));
        let g = r.grpc.as_ref().unwrap();
        assert_eq!((g.service.as_str(), g.method.as_str(), g.message.as_str()), ("a.B", "C", "{\"n\": ${n}}\n"));
        assert_eq!((g.protos.as_slice(), g.plaintext), (&["p/a.proto".to_string()][..], true));
        // An empty section is none; unknown keys in it are an error.
        assert_eq!(read_files(&files("kind: grpc\ngrpc: {}\n"), "x").unwrap().collection.requests[0].grpc, None);
        let err = read_files(&files("kind: grpc\ngrpc:\n  tls: true\n"), "x").unwrap_err();
        assert!(format!("{err:#}").contains("tls"), "{err:#}");
    }

    #[test]
    fn sync_plan_keeps_extensions_and_removes_stale_files() {
        let files: Files = [("hitop.yaml", "m"), ("requests/a.yaml", "a"), ("requests/n/b.yaml", "b"), ("environments/dev.yaml", "d")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let existing: Vec<String> = ["hitop.yaml", "requests/a.yml", "requests/old.yaml", "requests/.keep", "environments/prod.yaml", "README.md"].map(String::from).to_vec();
        let plan = plan_sync(&existing, &files).unwrap();
        assert_eq!(plan.write.keys().collect::<Vec<_>>(), ["environments/dev.yaml", "hitop.yaml", "requests/a.yml", "requests/n/b.yaml"]);
        assert_eq!(plan.remove, ["requests/old.yaml", "environments/prod.yaml"]);
    }
}
