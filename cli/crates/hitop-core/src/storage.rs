use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

use crate::model::{Collection, HistoryItem};

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
