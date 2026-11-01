//! `hitop globals show|set|unset|import|export [-c <folder>]`: the globals
//! scope, below every collection's variables. Global storage keeps it in
//! `globals.json`, a collection folder in `globals.yaml`. Secret globals:
//! `hitop secret set <name> --globals`.
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;

use hitop_core::{format, model::Environment, postman, secrets, storage};

use super::source::Source;

#[derive(Subcommand)]
pub enum GlobalsCmd {
    /// Show the globals (secret values as ••••)
    Show(Where),
    /// Set a global
    Set {
        key: String,
        value: String,
        #[command(flatten)]
        at: Where,
    },
    /// Remove a global
    Unset {
        key: String,
        #[command(flatten)]
        at: Where,
    },
    /// Import globals: a Postman globals (or environment) file, or HITOP's export.
    /// Imported values win; secret values go to the OS keychain
    Import {
        file: PathBuf,
        #[command(flatten)]
        at: Where,
    },
    /// Export the globals as JSON (secret names only, never their values)
    Export {
        /// Output file (stdout when left out)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        at: Where,
    },
}

#[derive(Args)]
pub struct Where {
    /// A collection folder's globals.yaml instead of global storage
    #[arg(short = 'c', long, value_name = "FOLDER")]
    collection: Option<String>,
}

/// Where the globals live: global storage, or a folder's `globals.yaml`.
pub struct Place {
    pub workspace: String,
    pub folder: Option<PathBuf>,
}

impl Place {
    pub fn of(collection: Option<&str>) -> Result<Place> {
        match collection {
            None => Ok(Place { workspace: secrets::GLOBAL_WORKSPACE.into(), folder: None }),
            Some(c) => {
                let source = Source::open(c)?;
                let Some(dir) = source.folder() else {
                    bail!("'{c}' is in global storage, which has one set of globals: leave out -c");
                };
                Ok(Place { workspace: source.workspace(), folder: Some(dir.to_path_buf()) })
            }
        }
    }

    pub fn load(&self) -> Result<Environment> {
        match &self.folder {
            Some(dir) => Ok(format::read_globals(dir)?.unwrap_or_else(Environment::globals)),
            None => storage::load_globals(),
        }
    }

    pub fn save(&self, g: &Environment) -> Result<()> {
        match &self.folder {
            Some(dir) => format::write_globals(dir, g),
            None => storage::save_globals(g),
        }
    }

    fn label(&self) -> String {
        match &self.folder {
            Some(dir) => dir.join(format::GLOBALS).display().to_string(),
            None => "globals".into(),
        }
    }
}

/// HITOP's globals export: `{variables, secrets}` without secret values.
pub fn exportable(g: &Environment) -> serde_json::Value {
    let variables: std::collections::BTreeMap<_, _> = g.variables.iter().filter(|(k, _)| !g.secrets.contains(k)).collect();
    let mut out = serde_json::json!({ "variables": variables });
    if !g.secrets.is_empty() {
        out["secrets"] = serde_json::json!(g.secrets);
    }
    out
}

/// Adds globals from a file (Postman or HITOP JSON) and stores their secret
/// values in the keychain. Returns how many variables came in.
pub fn import_into(place: &Place, text: &str) -> Result<usize> {
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).context("not JSON")?;
    let imported = if postman::detect(&v).is_some_and(|k| matches!(k, postman::Kind::Environment | postman::Kind::Globals)) {
        postman::import_environment(text)?
    } else if let Some(vars) = v["variables"].as_object() {
        let mut e = Environment::globals();
        for (k, val) in vars {
            e.variables.insert(k.clone(), val.as_str().map(str::to_string).unwrap_or_else(|| if val.is_null() { String::new() } else { val.to_string() }));
        }
        e.secrets = v["secrets"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
        for s in &e.secrets {
            e.variables.remove(s);
        }
        postman::ImportedEnvironment { environment: e, secret_values: vec![], kind: postman::Kind::Globals }
    } else {
        bail!("not a Postman globals file or HITOP's globals export (no `values` or `variables`)");
    };
    let mut g = place.load()?;
    let values = postman::merge_into_globals(&mut g, &imported);
    place.save(&g)?;
    store_secret_values(place, &values);
    Ok(imported.environment.variables.len() + imported.environment.secrets.len())
}

/// Puts imported globals' secret values in the keychain
/// (`<workspace>/global/<name>`), or says how to give them.
pub fn store_secret_values(place: &Place, values: &[(String, String)]) {
    if values.is_empty() {
        return;
    }
    let store = super::secret::store();
    for (name, value) in values {
        if let Err(e) = store.set(&secrets::account(&place.workspace, secrets::GLOBALS_ENV, name), value) {
            let c = place.folder.as_ref().map(|d| format!(" -c {}", d.display())).unwrap_or_default();
            eprintln!(
                "{} global secret '{name}' not stored ({e}); set {} or run: hitop secret set {name} --globals{c}",
                "note:".yellow(),
                secrets::env_var(secrets::GLOBALS_ENV, name)
            );
        }
    }
}

pub fn run(cmd: GlobalsCmd) -> Result<()> {
    match cmd {
        GlobalsCmd::Show(at) => {
            let place = Place::of(at.collection.as_deref())?;
            let g = place.load()?;
            if g.variables.is_empty() && g.secrets.is_empty() {
                println!("{}", "No globals. Add one with: hitop globals set <name> <value>".dimmed());
            }
            let mut keys: Vec<_> = g.variables.keys().filter(|k| !g.secrets.contains(k)).collect();
            keys.sort();
            for k in keys {
                println!("  {} = {}", k.bold(), g.variables[k]);
            }
            for k in &g.secrets {
                println!("  {} = {} {}", k.bold(), secrets::MASK, "(secret: hitop secret ls --globals)".dimmed());
            }
        }
        GlobalsCmd::Set { key, value, at } => {
            let place = Place::of(at.collection.as_deref())?;
            let mut g = place.load()?;
            if g.secrets.contains(&key) {
                bail!("'{key}' is a secret global; set it with: hitop secret set {key} --globals");
            }
            g.variables.insert(key.clone(), value.clone());
            place.save(&g)?;
            println!("{} Set {}={} in {}", "✓".green(), key, value, place.label());
        }
        GlobalsCmd::Unset { key, at } => {
            let place = Place::of(at.collection.as_deref())?;
            let mut g = place.load()?;
            let was_secret = g.secrets.contains(&key);
            if g.variables.remove(&key).is_none() && !was_secret {
                bail!("there is no global '{key}'");
            }
            g.secrets.retain(|s| s != &key);
            place.save(&g)?;
            if was_secret {
                let _ = super::secret::store().delete(&secrets::account(&place.workspace, secrets::GLOBALS_ENV, &key));
            }
            println!("{} Removed {} from {}", "✓".green(), key, place.label());
        }
        GlobalsCmd::Import { file, at } => {
            let place = Place::of(at.collection.as_deref())?;
            let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let n = import_into(&place, &text).with_context(|| file.display().to_string())?;
            println!("{} Imported {n} global{} into {}", "✓".green(), if n == 1 { "" } else { "s" }, place.label());
        }
        GlobalsCmd::Export { output, at } => {
            let place = Place::of(at.collection.as_deref())?;
            let text = serde_json::to_string_pretty(&exportable(&place.load()?))?;
            match output {
                Some(path) => {
                    std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
                    println!("{} Exported the globals to {}", "✓".green(), path.display());
                }
                None => println!("{text}"),
            }
        }
    }
    Ok(())
}
