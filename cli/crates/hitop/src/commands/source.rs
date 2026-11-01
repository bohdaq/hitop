//! A collection given on the command line: a name in global storage, or a
//! path to a collection folder (`hitop run ./my-api`).
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use hitop_core::{
    format,
    model::{Collection, Environment},
    storage,
};

enum Origin {
    Global { all: Vec<Collection>, index: usize },
    Folder(PathBuf),
}

pub struct Source {
    pub collection: Collection,
    /// The folder's environments; None for a collection in global storage.
    pub environments: Option<Vec<Environment>>,
    origin: Origin,
}

fn looks_like_path(arg: &str) -> bool {
    arg.contains('/') || arg.contains('\\') || arg.starts_with('.') || arg.starts_with('~')
}

impl Source {
    /// A folder with `hitop.yaml` wins; then a collection with that name;
    /// an argument that looks like a path but has no `hitop.yaml` is an
    /// error rather than a name lookup.
    pub fn open(arg: &str) -> Result<Source> {
        let path = Path::new(arg);
        if format::is_folder(path) {
            let folder = format::read_folder(path)?;
            return Ok(Source {
                collection: folder.collection,
                environments: Some(folder.environments),
                origin: Origin::Folder(path.to_path_buf()),
            });
        }
        let all = storage::load_collections()?;
        if let Some(index) = all.iter().position(|c| c.name == arg) {
            return Ok(Source { collection: all[index].clone(), environments: None, origin: Origin::Global { all, index } });
        }
        if looks_like_path(arg) || path.is_dir() {
            bail!("{arg} is not a collection folder (no hitop.yaml; create one with `hitop init {arg}`)");
        }
        bail!("collection '{arg}' not found")
    }

    /// Saves the collection where it came from. A folder gets its
    /// `hitop.yaml` and request files rewritten; its environments are left alone.
    pub fn save(&self) -> Result<()> {
        match &self.origin {
            Origin::Folder(dir) => format::write_collection(dir, &self.collection),
            Origin::Global { all, index } => {
                let mut all = all.clone();
                all[*index] = self.collection.clone();
                storage::save_collections(&all)
            }
        }
    }

    /// The environment to use: for a folder, only its own environments and
    /// only when `--env` names one; otherwise the global ones (`--env` or
    /// the active environment).
    pub fn environment(&self, name: Option<&str>) -> Result<Option<Environment>> {
        match (&self.environments, name) {
            (Some(_), None) => Ok(None),
            (Some(envs), Some(n)) => match envs.iter().find(|e| e.name == n) {
                Some(e) => Ok(Some(e.clone())),
                None => {
                    let names: Vec<&str> = envs.iter().map(|e| e.name.as_str()).collect();
                    bail!(
                        "environment '{n}' not found in this folder's environments/ ({})",
                        if names.is_empty() { "none".to_string() } else { names.join(", ") }
                    )
                }
            },
            (None, name) => Ok(storage::load_environments()?.pick(name)?.cloned()),
        }
    }

    /// The key under which `contexts.json` keeps this collection's script
    /// context.
    pub fn context_key(&self) -> String {
        match &self.origin {
            Origin::Global { .. } => self.collection.name.clone(),
            Origin::Folder(dir) => {
                let abs = dir.canonicalize().unwrap_or_else(|_| dir.clone());
                format!("folder:{}", abs.display())
            }
        }
    }
}
