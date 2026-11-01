//! `hitop init` and `hitop convert`: collection folders on disk.
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;
use hitop_core::{export, format, model::Environment, storage::Environments};

#[derive(Args)]
pub struct InitArgs {
    /// Directory to create the collection in (default: the current one)
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Collection name (default: the directory's name)
    #[arg(long)]
    name: Option<String>,
}

pub fn run_init(args: InitArgs) -> Result<()> {
    let folder = format::init(&args.dir, args.name.as_deref())?;
    println!("{} Created collection '{}' in {}", "✓".green(), folder.collection.name.bold(), args.dir.display());
    println!("  hitop.yaml, requests/get-example.yaml, environments/local.yaml");
    println!("  Run it: hitop run {}", shell_path(&args.dir));
    Ok(())
}

fn shell_path(dir: &Path) -> String {
    let s = dir.display().to_string();
    if s.contains('/') || s == "." { s } else { format!("./{s}") }
}

#[derive(Args)]
pub struct ConvertArgs {
    /// A HITOP JSON export, or a collection folder
    from: PathBuf,
    /// A collection folder to write, or a JSON file
    to: PathBuf,
    /// The collection to take when the JSON has more than one
    #[arg(short = 'c', long)]
    collection: Option<String>,
    /// Environments JSON (`environments.json` from hitop's storage, or an
    /// array): read into the folder's environments/, or written from it
    #[arg(long, value_name = "FILE")]
    environments: Option<PathBuf>,
    /// Globals JSON (`globals.json` from hitop's storage): read into the
    /// folder's globals.yaml, or written from it
    #[arg(long, value_name = "FILE")]
    globals: Option<PathBuf>,
    /// Overwrite an existing collection folder or JSON file
    #[arg(long)]
    force: bool,
}

pub fn run_convert(args: ConvertArgs) -> Result<()> {
    if format::is_folder(&args.from) {
        folder_to_json(&args)
    } else if args.from.is_file() {
        json_to_folder(&args)
    } else if args.from.is_dir() {
        bail!("{} is not a collection folder (no hitop.yaml)", args.from.display())
    } else {
        bail!("{} not found", args.from.display())
    }
}

fn refuse_overwrite(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!("{} exists; pass --force to overwrite it", path.display());
    }
    Ok(())
}

fn folder_to_json(args: &ConvertArgs) -> Result<()> {
    let folder = format::read_folder(&args.from)?;
    refuse_overwrite(&args.to, args.force)?;
    std::fs::write(&args.to, export::to_hitop(std::slice::from_ref(&folder.collection))? + "\n")
        .with_context(|| format!("writing {}", args.to.display()))?;
    println!(
        "{} Wrote '{}' ({} requests) to {}",
        "✓".green(),
        folder.collection.name.bold(),
        folder.collection.requests.len(),
        args.to.display()
    );
    if let Some(path) = &args.environments {
        refuse_overwrite(path, args.force)?;
        let envs = Environments { active: None, environments: folder.environments.clone() };
        std::fs::write(path, serde_json::to_string_pretty(&envs)? + "\n").with_context(|| format!("writing {}", path.display()))?;
        println!("{} Wrote {} environment(s) to {}", "✓".green(), envs.environments.len(), path.display());
    }
    if let Some(path) = &args.globals {
        refuse_overwrite(path, args.force)?;
        let g = folder.globals.clone().unwrap_or_else(Environment::globals);
        std::fs::write(path, serde_json::to_string_pretty(&g)? + "\n").with_context(|| format!("writing {}", path.display()))?;
        println!("{} Wrote the globals to {}", "✓".green(), path.display());
    }
    Ok(())
}

fn json_to_folder(args: &ConvertArgs) -> Result<()> {
    let raw = std::fs::read_to_string(&args.from).with_context(|| format!("reading {}", args.from.display()))?;
    let mut collections = export::from_hitop(&raw).with_context(|| format!("{} is not a HITOP JSON export", args.from.display()))?;
    let collection = match (&args.collection, collections.len()) {
        (Some(name), _) => {
            let i = collections.iter().position(|c| &c.name == name).with_context(|| format!("no collection '{name}' in {}", args.from.display()))?;
            collections.swap_remove(i)
        }
        (None, 1) => collections.remove(0),
        (None, 0) => bail!("{} has no collections", args.from.display()),
        (None, _) => {
            let names: Vec<_> = collections.iter().map(|c| c.name.as_str()).collect();
            bail!("{} has {} collections ({}); pick one with --collection", args.from.display(), names.len(), names.join(", "))
        }
    };
    let environments = match &args.environments {
        None => vec![],
        Some(path) => read_environments(path)?,
    };
    for existing in [format::MANIFEST, "requests", "environments"] {
        refuse_overwrite(&args.to.join(existing), args.force)?;
    }
    let globals = match &args.globals {
        None => None,
        Some(path) => {
            refuse_overwrite(&args.to.join(format::GLOBALS), args.force)?;
            let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let mut v: serde_json::Value = serde_json::from_str(&raw).with_context(|| format!("{}", path.display()))?;
            if let Some(o) = v.as_object_mut() {
                o.insert("id".into(), hitop_core::model::GLOBALS_ID.into());
                o.insert("name".into(), hitop_core::model::GLOBALS_ID.into());
            }
            Some(serde_json::from_value::<Environment>(v).with_context(|| format!("{}", path.display()))?)
        }
    };
    let folder = format::Folder { collection, environments, globals };
    if args.environments.is_some() {
        format::write_folder(&args.to, &folder)?;
    } else {
        format::write_collection(&args.to, &folder.collection)?;
        if let Some(g) = &folder.globals {
            format::write_globals(&args.to, g)?;
        }
    }
    println!(
        "{} Wrote '{}' ({} requests{}) to {}",
        "✓".green(),
        folder.collection.name.bold(),
        folder.collection.requests.len(),
        if args.environments.is_some() { format!(", {} environments", folder.environments.len()) } else { String::new() },
        args.to.display()
    );
    Ok(())
}

fn read_environments(path: &Path) -> Result<Vec<Environment>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw).with_context(|| format!("{}", path.display()))?;
    Ok(if value.is_array() {
        serde_json::from_value(value)?
    } else {
        serde_json::from_value::<Environments>(value).with_context(|| format!("{}", path.display()))?.environments
    })
}
