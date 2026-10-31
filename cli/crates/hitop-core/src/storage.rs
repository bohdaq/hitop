use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

use crate::model::{Collection, Environment, HistoryItem};

const MAX_HISTORY: usize = 50;

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("cannot determine config directory")?;
    let dir = base.join("hitop");
    fs::create_dir_all(&dir)?;
    Ok(dir)
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

fn contexts_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("contexts.json"))
}

pub fn load_collections() -> Result<Vec<Collection>> {
    let path = collections_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(&path)?;
    let cols: Vec<Collection> = serde_json::from_str(&data)?;
    Ok(cols)
}

pub fn save_collections(collections: &[Collection]) -> Result<()> {
    let path = collections_path()?;
    let data = serde_json::to_string_pretty(collections)?;
    fs::write(path, data)?;
    Ok(())
}

pub fn load_history() -> Result<Vec<HistoryItem>> {
    let path = history_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

pub fn save_history(history: &[HistoryItem]) -> Result<()> {
    let path = history_path()?;
    let trimmed = if history.len() > MAX_HISTORY {
        &history[history.len() - MAX_HISTORY..]
    } else {
        history
    };
    fs::write(path, serde_json::to_string_pretty(trimmed)?)?;
    Ok(())
}

pub fn load_contexts() -> Result<serde_json::Value> {
    let path = contexts_path()?;
    if !path.exists() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

pub fn save_contexts(contexts: &serde_json::Value) -> Result<()> {
    let path = contexts_path()?;
    fs::write(path, serde_json::to_string_pretty(contexts)?)?;
    Ok(())
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
    Ok(serde_json::from_str(&fs::read_to_string(&path)?)?)
}

pub fn save_environments(envs: &Environments) -> Result<()> {
    fs::write(environments_path()?, serde_json::to_string_pretty(envs)?)?;
    Ok(())
}
