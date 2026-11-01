//! Collection folders (format v1, `spec/collection-format.md`):
//!
//! ```text
//! my-api/
//!   hitop.yaml            format, name, variables, default auth, request order
//!   requests/*.yaml       one request per file; subfolders are folders
//!   environments/*.yaml   variables; secret ones have no value in the file
//!   globals.yaml          optional: the globals scope, below the collection's variables
//! ```
//!
//! Reading a folder gives the same [`Collection`] and [`Environment`]
//! model the JSON storage uses, and writing it back gives the same files:
//! the conversion is lossless both ways (see the spec for the two
//! normalizations).
mod convert;
mod doc;
pub mod files;
pub mod names;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub use convert::infer_body_type;
use convert::{check_extra, COLLECTION_KEYS};
use doc::{CollectionDoc, EnvironmentDoc, RequestDoc, FORMAT_VERSION};

use crate::model::{Collection, Environment, GLOBALS_ID};

pub const MANIFEST: &str = "hitop.yaml";
const REQUESTS: &str = "requests";
const ENVIRONMENTS: &str = "environments";
/// The globals scope at the folder's root (id and name [`GLOBALS_ID`]).
pub const GLOBALS: &str = "globals.yaml";

/// A collection folder in memory.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folder {
    pub collection: Collection,
    pub environments: Vec<Environment>,
    /// `globals.yaml`. On write, `None` leaves an existing file alone (so a
    /// writer that doesn't know globals can't drop them); `Some` writes it.
    pub globals: Option<Environment>,
}

/// True when `dir` holds a collection folder.
pub fn is_folder(dir: &Path) -> bool {
    dir.join(MANIFEST).is_file()
}

fn yaml<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(yaml_serde::to_string(value)?)
}

fn parse<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    yaml_serde::from_str(&text).with_context(|| format!("{}", path.display()))
}

fn is_yaml(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("yaml" | "yml"))
}

/// Every `.yaml`/`.yml` file under `dir`, by its `/`-separated path
/// without the extension. Hidden entries and symlinks are skipped.
fn yaml_files(dir: &Path, recursive: bool) -> Result<BTreeMap<String, PathBuf>> {
    fn walk(base: &Path, dir: &Path, recursive: bool, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() && recursive {
                walk(base, &path, recursive, out)?;
            } else if meta.is_file() && is_yaml(&path) {
                let rel = path.strip_prefix(base).unwrap_or(&path).with_extension("");
                let key = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                if let Some(prev) = out.insert(key.clone(), path.clone()) {
                    bail!("{} and {} are the same request; keep one", prev.display(), path.display());
                }
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    if dir.is_dir() {
        walk(dir, dir, recursive, &mut out)?;
    }
    Ok(out)
}

/// Reads a collection folder.
pub fn read_folder(dir: &Path) -> Result<Folder> {
    let manifest_path = dir.join(MANIFEST);
    if !manifest_path.is_file() {
        bail!("{} is not a collection folder (no {MANIFEST}; create one with `hitop init`)", dir.display());
    }
    let manifest: CollectionDoc = parse(&manifest_path)?;
    match manifest.format {
        None => bail!("{}: `format: {FORMAT_VERSION}` is missing", manifest_path.display()),
        Some(v) if v > FORMAT_VERSION => bail!("{}: format {v} needs a newer hitop", manifest_path.display()),
        Some(_) => {}
    }
    check_extra(&manifest.extra, COLLECTION_KEYS, MANIFEST)?;

    let files = yaml_files(&dir.join(REQUESTS), true)?;
    names::check_case_clash(files.keys())?;
    let mut ordered: Vec<String> = vec![];
    let mut seen = BTreeSet::new();
    for entry in &manifest.order {
        names::check_rel_path(entry).with_context(|| format!("{MANIFEST}: order"))?;
        if files.contains_key(entry) && seen.insert(entry.clone()) {
            ordered.push(entry.clone());
        }
    }
    ordered.extend(files.keys().filter(|k| !seen.contains(*k)).cloned());

    let mut requests = Vec::with_capacity(ordered.len());
    for file in &ordered {
        names::check_rel_path(file).with_context(|| format!("{}", files[file].display()))?;
        let doc: RequestDoc = parse(&files[file])?;
        requests.push(convert::doc_to_request(doc, file)?);
    }
    // Requests whose file is the default for their name don't keep it.
    let pairs: Vec<(&str, &str)> = requests.iter().zip(&ordered).map(|(r, f)| (r.name.as_str(), f.as_str())).collect();
    let explicit = names::explicit_files(&pairs);
    for (r, f) in requests.iter_mut().zip(explicit) {
        r.file = f;
    }

    let name = match manifest.name {
        Some(n) => n,
        None => dir.canonicalize().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default(),
    };
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
    for (stem, path) in yaml_files(&dir.join(ENVIRONMENTS), false)? {
        let doc: EnvironmentDoc = parse(&path)?;
        environments.push(convert::doc_to_environment(doc, &stem).with_context(|| format!("{}", path.display()))?);
    }
    let globals = read_globals(dir)?;
    Ok(Folder { collection, environments, globals })
}

/// `globals.yaml` as the globals scope: an environment file whose id and
/// name are always [`GLOBALS_ID`].
fn doc_to_globals(doc: EnvironmentDoc) -> Result<Environment> {
    let mut g = convert::doc_to_environment(doc, GLOBALS_ID)?;
    g.id = GLOBALS_ID.into();
    g.name = GLOBALS_ID.into();
    Ok(g)
}

/// The text of `globals.yaml`.
fn globals_file(globals: &Environment) -> Result<String> {
    let g = Environment { id: GLOBALS_ID.into(), name: GLOBALS_ID.into(), ..globals.clone() };
    yaml(&convert::environment_to_doc(&g, GLOBALS_ID)?)
}

/// Reads a folder's `globals.yaml` (`None` when it has none).
pub fn read_globals(dir: &Path) -> Result<Option<Environment>> {
    for file in [GLOBALS, "globals.yml"] {
        let path = dir.join(file);
        if path.is_file() {
            return Ok(Some(doc_to_globals(parse(&path)?).with_context(|| format!("{}", path.display()))?));
        }
    }
    Ok(None)
}

/// Writes a folder's `globals.yaml` (an existing `globals.yml` keeps its name).
pub fn write_globals(dir: &Path, globals: &Environment) -> Result<()> {
    let text = globals_file(globals)?;
    let path = if dir.join("globals.yml").is_file() && !dir.join(GLOBALS).is_file() { dir.join("globals.yml") } else { dir.join(GLOBALS) };
    write_if_changed(&path, &text)?;
    Ok(())
}

/// Reads one environment file (spec section 5) from anywhere, as
/// `hitop run --env-file` does. Its name defaults to the file name.
pub fn read_environment_file(path: &Path) -> Result<Environment> {
    if !is_yaml(path) {
        bail!("{}: an environment file is .yaml or .yml", path.display());
    }
    let doc: EnvironmentDoc = parse(path)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    convert::doc_to_environment(doc, &stem).with_context(|| format!("{}", path.display()))
}

/// Writes only when the content changed, through a temporary file.
fn write_if_changed(path: &Path, content: &str) -> Result<bool> {
    if fs::read_to_string(path).is_ok_and(|old| old == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("yaml.tmp");
    fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Writes `files` (path under `sub` without extension → YAML) and removes
/// the YAML files there that are no longer in it. An existing `.yml` file
/// keeps its extension.
fn sync_dir(dir: &Path, sub: &str, files: &BTreeMap<String, String>, recursive: bool) -> Result<()> {
    let base = dir.join(sub);
    let existing = yaml_files(&base, recursive)?;
    for (key, content) in files {
        let path = match existing.get(key) {
            Some(p) => p.clone(),
            None => base.join(format!("{key}.yaml")),
        };
        write_if_changed(&path, content)?;
    }
    for (key, path) in &existing {
        if !files.contains_key(key) {
            fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    if recursive && base.is_dir() {
        remove_empty_dirs(&base, &base)?;
    }
    Ok(())
}

fn remove_empty_dirs(base: &Path, dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if fs::symlink_metadata(&path)?.is_dir() {
            remove_empty_dirs(base, &path)?;
        }
    }
    if dir != base && fs::read_dir(dir)?.next().is_none() {
        fs::remove_dir(dir)?;
    }
    Ok(())
}

/// The files of a collection: `hitop.yaml` and `requests/` contents.
fn collection_files(collection: &Collection) -> Result<(String, BTreeMap<String, String>)> {
    check_extra(&collection.extra, COLLECTION_KEYS, &format!("collection '{}'", collection.name))?;
    let input: Vec<(&str, Option<&str>)> = collection.requests.iter().map(|r| (r.name.as_str(), r.file.as_deref())).collect();
    let files = names::assign_files(&input)?;
    let mut out = BTreeMap::new();
    for (r, file) in collection.requests.iter().zip(&files) {
        out.insert(file.clone(), yaml(&convert::request_to_doc(r, file)?)?);
    }
    let mut sorted = files.clone();
    sorted.sort();
    let manifest = CollectionDoc {
        format: Some(FORMAT_VERSION),
        name: Some(collection.name.clone()),
        id: (collection.id != names::collection_id(&collection.name)).then_some(collection.id),
        workspace: collection.workspace.clone(),
        variables: collection.variables.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        auth: collection.auth.clone(),
        order: if sorted == files { vec![] } else { files },
        extra: collection.extra.clone(),
    };
    Ok((yaml(&manifest)?, out))
}

fn environment_files(environments: &[Environment]) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut used = BTreeSet::new();
    for e in environments {
        let base = names::slug(&e.name);
        let mut stem = base.clone();
        let mut n = 2;
        while !used.insert(stem.clone()) {
            stem = format!("{base}-{n}");
            n += 1;
        }
        out.insert(stem.clone(), yaml(&convert::environment_to_doc(e, &stem)?)?);
    }
    Ok(out)
}

/// Writes `hitop.yaml` and `requests/`; `environments/` is left alone.
/// Request files no longer in the collection are removed.
pub fn write_collection(dir: &Path, collection: &Collection) -> Result<()> {
    let (manifest, requests) = collection_files(collection)?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    sync_dir(dir, REQUESTS, &requests, true)?;
    write_if_changed(&dir.join(MANIFEST), &manifest)?;
    Ok(())
}

/// Writes `environments/`, removing environment files no longer listed.
pub fn write_environments(dir: &Path, environments: &[Environment]) -> Result<()> {
    let files = environment_files(environments)?;
    sync_dir(dir, ENVIRONMENTS, &files, false)
}

/// Marks `name` as a secret in the environment called `env`, editing only
/// that environment's file: the variable becomes `{secret: true}` and any
/// value it had is dropped. False when it already was a secret.
pub fn mark_secret(dir: &Path, env: &str, name: &str) -> Result<bool> {
    for (stem, path) in yaml_files(&dir.join(ENVIRONMENTS), false)? {
        let doc: EnvironmentDoc = parse(&path)?;
        let mut e = convert::doc_to_environment(doc, &stem).with_context(|| format!("{}", path.display()))?;
        if e.name != env {
            continue;
        }
        if e.secrets.iter().any(|s| s == name) {
            return Ok(false);
        }
        e.variables.remove(name);
        e.secrets.push(name.to_string());
        e.secrets.sort();
        write_if_changed(&path, &yaml(&convert::environment_to_doc(&e, &stem)?)?)?;
        return Ok(true);
    }
    bail!("environment '{env}' not found in {}", dir.join(ENVIRONMENTS).display())
}

/// Writes the whole folder.
pub fn write_folder(dir: &Path, folder: &Folder) -> Result<()> {
    // Build everything first, so a model that can't be written leaves the
    // folder untouched.
    collection_files(&folder.collection)?;
    environment_files(&folder.environments)?;
    if let Some(g) = &folder.globals {
        globals_file(g)?;
    }
    write_collection(dir, &folder.collection)?;
    write_environments(dir, &folder.environments)?;
    if let Some(g) = &folder.globals {
        write_globals(dir, g)?;
    }
    Ok(())
}

/// The starter folder `hitop init` writes.
pub fn starter(name: &str) -> Folder {
    use crate::model::{Header, Request};
    let request = Request {
        id: names::request_id("get-example"),
        name: "Get example".into(),
        url: "${base}/".into(),
        method: "GET".into(),
        headers: vec![Header::new("Accept", "text/html")],
        post_request_script: "assert.status(200);\n".into(),
        ..Default::default()
    };
    Folder {
        collection: Collection {
            id: names::collection_id(name),
            name: name.into(),
            requests: vec![request],
            variables: [("base".to_string(), "https://example.com".to_string())].into(),
            ..Default::default()
        },
        environments: vec![Environment {
            id: "local".into(),
            name: "local".into(),
            variables: [("base".to_string(), "http://localhost:8080".to_string())].into(),
            ..Default::default()
        }],
        globals: None,
    }
}

/// Creates a collection folder in `dir` (made if missing). Refuses when a
/// collection, `requests/` or `environments/` is already there.
pub fn init(dir: &Path, name: Option<&str>) -> Result<Folder> {
    for existing in [MANIFEST, REQUESTS, ENVIRONMENTS] {
        if dir.join(existing).exists() {
            bail!("{} already has {existing}; not overwriting it", dir.display());
        }
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let name = match name {
        Some(n) => n.to_string(),
        None => dir
            .canonicalize()?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "my-api".into()),
    };
    let mut folder = starter(&name);
    folder.collection.workspace = Some(crate::secrets::new_workspace_id());
    write_folder(dir, &folder)?;
    Ok(folder)
}

#[cfg(test)]
mod tests;
