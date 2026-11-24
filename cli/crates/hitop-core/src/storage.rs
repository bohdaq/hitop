#![cfg(feature = "native")]
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

use crate::model::{Collection, Environment, HistoryItem};

const MAX_HISTORY: usize = 50;

/// hitop's settings folder (`~/.config/hitop`, `~/Library/Application Support/hitop`), created when missing.
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("cannot determine config directory")?;
    let dir = base.join("hitop");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `<file>.tmp` next to a file: the whole new text, there only while the file is being written.
fn journal(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The file's JSON. A write that was cut off is finished from its journal; a file that can't be
/// read is an error, never an empty value.
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    match serde_json::from_str(&text) {
        Ok(v) => Ok(v),
        Err(e) => {
            let tmp = journal(path);
            if serde_json::from_str::<serde_json::Value>(&text).is_err() {
                if let Some((v, saved)) = fs::read_to_string(&tmp).ok().and_then(|t| serde_json::from_str(&t).ok().map(|v| (v, t))) {
                    fs::write(path, saved)?;
                    let _ = fs::remove_file(&tmp);
                    return Ok(v);
                }
            }
            Err(e).with_context(|| format!("{} can't be read (it is kept as it is)", path.display()))
        }
    }
}

/// Writes a file in place (the Microsoft Store build shares it with the CLI only that way), with
/// the new text in a journal first. A file that isn't `T` any more is copied to
/// `<file>.damaged-<time>` before it is replaced.
fn write_json<T: serde::Serialize + serde::de::DeserializeOwned>(path: &Path, text: &str) -> Result<()> {
    if let Ok(old) = fs::read(path) {
        if !old.is_empty() && serde_json::from_slice::<T>(&old).is_err() {
            let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let mut name = path.file_name().unwrap_or_default().to_os_string();
            name.push(format!(".damaged-{ms}"));
            fs::write(path.with_file_name(name), old)?;
        }
    }
    let tmp = journal(path);
    let mut f = fs::File::create(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    let _ = fs::remove_file(&tmp);
    Ok(())
}

fn collections_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("collections.json"))
}

fn history_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("history.json"))
}

fn environments_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("environments.json"))
}

fn globals_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("globals.json"))
}

/// The globals of global storage (`globals.json`); empty when there is none.
/// Secret values aren't in it (they are in the keychain as
/// `global/global/<name>`, see `secrets`).
pub fn load_globals() -> Result<Environment> {
    let path = globals_path()?;
    if !path.exists() {
        return Ok(Environment::globals());
    }
    let mut v: serde_json::Value = read_json(&path)?;
    if let Some(o) = v.as_object_mut() {
        o.insert("id".into(), crate::model::GLOBALS_ID.into());
        o.insert("name".into(), crate::model::GLOBALS_ID.into());
    }
    Ok(serde_json::from_value(v)?)
}

pub fn save_globals(globals: &Environment) -> Result<()> {
    write_json::<serde_json::Map<String, serde_json::Value>>(&globals_path()?, &serde_json::to_string_pretty(globals)?)
}

fn contexts_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("contexts.json"))
}

pub fn load_collections() -> Result<Vec<Collection>> {
    let path = collections_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    read_json(&path)
}

pub fn save_collections(collections: &[Collection]) -> Result<()> {
    let path = collections_path()?;
    write_json::<Vec<Collection>>(&path, &serde_json::to_string_pretty(collections)?)
}

pub fn load_history() -> Result<Vec<HistoryItem>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    Ok(read_json(&path).unwrap_or_default())
}

pub fn save_history(history: &[HistoryItem]) -> Result<()> {
    let path = history_path()?;
    let trimmed = if history.len() > MAX_HISTORY {
        &history[history.len() - MAX_HISTORY..]
    } else {
        history
    };
    write_json::<Vec<HistoryItem>>(&path, &serde_json::to_string_pretty(trimmed)?)
}

pub fn load_contexts() -> Result<serde_json::Value> {
    let path = contexts_path()?;
    if !path.exists() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    Ok(read_json(&path).unwrap_or_default())
}

pub fn save_contexts(contexts: &serde_json::Value) -> Result<()> {
    let path = contexts_path()?;
    write_json::<serde_json::Value>(&path, &serde_json::to_string_pretty(contexts)?)
}

/// Environments and the name of the active one (`environments.json`).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Environments {
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub environments: Vec<Environment>,
}

impl Environments {
    /// The environment named `name`, or the active one when `name` is None.
    pub fn pick(&self, name: Option<&str>) -> Result<Option<&Environment>> {
        match name {
            Some(n) => self
                .environments
                .iter()
                .find(|e| e.name == n)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("environment '{}' not found", n)),
            None => Ok(self.active.as_ref().and_then(|id| self.environments.iter().find(|e| &e.id == id))),
        }
    }
}

pub fn load_environments() -> Result<Environments> {
    let path = environments_path()?;
    if !path.exists() {
        return Ok(Environments::default());
    }
    read_json(&path)
}

pub fn save_environments(envs: &Environments) -> Result<()> {
    write_json::<Environments>(&environments_path()?, &serde_json::to_string_pretty(envs)?)
}

#[cfg(test)]
mod safety {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hitop-storage-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_damaged_file_is_an_error_and_is_kept_when_replaced() {
        let d = dir("damaged");
        let p = d.join("collections.json");
        fs::write(&p, "[{\"id\": 1, \"na").unwrap();
        assert!(read_json::<Vec<Collection>>(&p).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "[{\"id\": 1, \"na");
        write_json::<Vec<Collection>>(&p, "[]").unwrap();
        let kept: Vec<_> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().path()).filter(|p| p.to_string_lossy().contains(".damaged-")).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read_to_string(&kept[0]).unwrap(), "[{\"id\": 1, \"na");
        // A good file is replaced without a copy, and no journal stays.
        write_json::<Vec<Collection>>(&p, "[ ]").unwrap();
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_write_that_was_cut_off_is_finished_from_its_journal() {
        let d = dir("journal");
        let p = d.join("environments.json");
        fs::write(&p, "{\"acti").unwrap();
        fs::write(journal(&p), "{\"active\": \"e1\", \"environments\": []}").unwrap();
        assert_eq!(read_json::<Environments>(&p).unwrap().active.as_deref(), Some("e1"));
        assert!(!journal(&p).exists());
        assert_eq!(read_json::<Environments>(&p).unwrap().active.as_deref(), Some("e1"));
        // A journal never wins over a good file.
        fs::write(journal(&p), "{\"active\": \"old\"}").unwrap();
        assert_eq!(read_json::<Environments>(&p).unwrap().active.as_deref(), Some("e1"));
        fs::remove_dir_all(&d).unwrap();
    }
}
