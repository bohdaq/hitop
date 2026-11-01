use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use colored::Colorize;
use serde_json::Value;
use std::path::{Path, PathBuf};

use hitop_core::{
    export, format,
    model::Collection,
    postman::{self, ImportReport, ImportedEnvironment},
    secrets, storage,
};

#[derive(ValueEnum, Clone)]
pub enum Format {
    Hitop,
    Postman,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct ImportArgs {
    #[command(subcommand)]
    kind: Option<ImportKind>,

    /// JSON files: HITOP or Postman collections, Postman environments and
    /// globals (detected from the content), or one OpenAPI spec
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Force the collection format (auto-detected if omitted)
    #[arg(long)]
    format: Option<Format>,

    /// Write a collection folder here (hitop.yaml, requests/, environments/)
    /// instead of global storage. Without a collection in the files, adds
    /// the environments to the folder already there.
    #[arg(long, value_name = "DIR")]
    to: Option<PathBuf>,

    /// With --to: replace the collection in a folder that already has one
    #[arg(long)]
    force: bool,
}

#[derive(Subcommand)]
pub enum ImportKind {
    /// An OpenAPI 3.0 or 3.1 spec (YAML or JSON, a file or an http(s) URL):
    /// a folder per tag, a request per operation
    Openapi(super::openapi::ImportOpenapiArgs),
}

#[derive(Args)]
pub struct ExportArgs {
    /// Collection name or folder path
    collection: String,

    /// Output format
    #[arg(long, default_value = "hitop")]
    format: Format,

    /// Output file (prints to stdout if omitted)
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
}

pub fn run_import(args: ImportArgs) -> Result<()> {
    if let Some(ImportKind::Openapi(a)) = args.kind {
        return super::openapi::run_import(a);
    }
    // An OpenAPI spec given like any other file.
    if args.format.is_none() {
        for file in &args.files {
            let raw = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
            if hitop_core::openapi::detect(&raw) {
                if args.files.len() > 1 {
                    bail!("{} is an OpenAPI spec: import it on its own (hitop import openapi {})", file.display(), file.display());
                }
                return super::openapi::import_text(&raw, None, args.to.as_deref(), args.force, false);
            }
        }
    }
    let mut collections: Vec<(Collection, ImportReport)> = vec![];
    let mut environments: Vec<ImportedEnvironment> = vec![];
    let mut globals: Vec<ImportedEnvironment> = vec![];
    for file in &args.files {
        let raw = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        let raw = raw.trim_start_matches('\u{feff}');
        let v: Value = serde_json::from_str(raw).with_context(|| format!("{} is not JSON", file.display()))?;
        let kind = match args.format {
            Some(Format::Postman) => Some(postman::Kind::Collection),
            Some(Format::Hitop) => None,
            None => postman::detect(&v),
        };
        match kind {
            Some(postman::Kind::Collection) => collections.push(postman::import_collection(raw).with_context(|| file.display().to_string())?),
            Some(postman::Kind::Environment) => environments.push(postman::import_environment(raw)?),
            Some(postman::Kind::Globals) => globals.push(postman::import_environment(raw)?),
            None => collections.extend(export::from_hitop(raw)?.into_iter().map(|c| (c, ImportReport::default()))),
        }
    }
    let workspace = match &args.to {
        Some(dir) => import_to_folder(dir, &mut collections, &mut environments, args.force)?,
        None => import_to_storage(&mut collections, &mut environments)?,
    };
    store_secrets(&workspace, &environments, args.to.as_deref());
    // Postman globals go to hitop's globals: the folder's globals.yaml, or
    // global storage's globals.json.
    if !globals.is_empty() {
        let place = super::globals::Place { workspace: workspace.clone(), folder: args.to.clone() };
        let mut g = place.load()?;
        let mut values = vec![];
        for imported in &globals {
            values.extend(postman::merge_into_globals(&mut g, imported));
        }
        place.save(&g)?;
        super::globals::store_secret_values(&place, &values);
        let n: usize = globals.iter().map(|i| i.environment.variables.len() + i.environment.secrets.len()).sum();
        let at = args.to.as_ref().map(|d| d.join(format::GLOBALS).display().to_string()).unwrap_or_else(|| "globals".into());
        println!("{} Imported {n} global{} into {at}", "✓".green(), if n == 1 { "" } else { "s" });
    }
    for (c, report) in &collections {
        print_report(&c.name, report);
    }
    Ok(())
}

pub(super) fn import_to_folder(dir: &Path, collections: &mut [(Collection, ImportReport)], environments: &mut [ImportedEnvironment], force: bool) -> Result<String> {
    if collections.len() > 1 {
        bail!("--to writes one collection; {} found in the files", collections.len());
    }
    let existing = format::is_folder(dir).then(|| format::read_folder(dir)).transpose()?;
    let collection = match collections.first_mut() {
        Some((c, _)) => {
            if existing.is_some() && !force {
                bail!("{} already holds a collection; use --force to replace it", dir.display());
            }
            if dir.exists() && existing.is_none() && std::fs::read_dir(dir)?.next().is_some() {
                bail!("{} is not empty and not a collection folder", dir.display());
            }
            // A folder keeps its workspace id, so secrets stay where they are.
            c.workspace = existing.as_ref().and_then(|f| f.collection.workspace.clone()).or_else(|| Some(secrets::new_workspace_id()));
            format::write_collection(dir, c)?;
            println!("{} Imported '{}' into {} ({} requests)", "✓".green(), c.name.bold(), dir.display(), c.requests.len());
            c.clone()
        }
        None => match &existing {
            Some(f) => f.collection.clone(),
            None => bail!("{} is not a collection folder; import a collection with the environments", dir.display()),
        },
    };
    if !environments.is_empty() {
        let mut all = existing.map(|f| f.environments).unwrap_or_default();
        for imported in environments.iter() {
            let env = imported.environment.clone();
            all.retain(|e| e.name != env.name);
            all.push(env);
        }
        format::write_environments(dir, &all)?;
        for e in environments.iter() {
            println!("{} Imported environment '{}' into {}", "✓".green(), e.environment.name.bold(), dir.join("environments").display());
        }
    }
    Ok(secrets::workspace_id(&collection, true))
}

pub(super) fn import_to_storage(collections: &mut [(Collection, ImportReport)], environments: &mut [ImportedEnvironment]) -> Result<String> {
    if !collections.is_empty() {
        let mut stored = storage::load_collections()?;
        for (col, _) in collections.iter_mut() {
            // Regenerate ID to avoid collisions
            col.id = timestamp_id();
            // If name collides, suffix it
            if stored.iter().any(|c| c.name == col.name) {
                col.name = format!("{} (Imported)", col.name);
            }
            println!("{} Imported '{}'", "✓".green(), col.name.bold());
            stored.push(col.clone());
        }
        storage::save_collections(&stored)?;
        println!("Imported {} collection(s).", collections.len());
    }
    if !environments.is_empty() {
        let mut envs = storage::load_environments()?;
        for imported in environments.iter_mut() {
            let e = &mut imported.environment;
            if envs.environments.iter().any(|x| x.name == e.name) {
                e.name = format!("{} (Imported)", e.name);
            }
            e.id = format!("{:x}", timestamp_id()) + &envs.environments.len().to_string();
            println!("{} Imported environment '{}'", "✓".green(), e.name.bold());
            envs.environments.push(e.clone());
        }
        storage::save_environments(&envs)?;
    }
    Ok(secrets::GLOBAL_WORKSPACE.into())
}

/// Puts imported secret values in the OS keychain. Without one, says how
/// to give each value instead; values never go to a file.
pub fn store_secrets(workspace: &str, environments: &[ImportedEnvironment], folder: Option<&Path>) {
    let pending: Vec<(&str, &str, &str)> = environments
        .iter()
        .flat_map(|e| e.secret_values.iter().map(move |(k, v)| (e.environment.name.as_str(), k.as_str(), v.as_str())))
        .collect();
    if pending.is_empty() {
        return;
    }
    let store = super::secret::store();
    let mut stored = 0;
    for (env, name, value) in pending {
        match store.set(&secrets::account(workspace, env, name), value) {
            Ok(()) => stored += 1,
            Err(e) => {
                let c = folder.map(|d| format!(" -c {}", d.display())).unwrap_or_default();
                eprintln!(
                    "{} secret '{name}' of '{env}' not stored ({e}); set {} or run: hitop secret set {name} --env {env}{c}",
                    "note:".yellow(),
                    secrets::env_var(env, name)
                );
            }
        }
    }
    if stored > 0 {
        println!("{} Stored {stored} secret value{} in the OS keychain", "✓".green(), if stored == 1 { "" } else { "s" });
    }
}

/// Lists what the import couldn't carry over: unsupported `pm.*` calls by
/// script line, auth types, body modes, dynamic variables.
fn print_report(name: &str, report: &ImportReport) {
    if report.is_empty() {
        return;
    }
    let scripts = report.script_issues().count();
    println!(
        "{} '{}': {} thing{} to check{}",
        "!".yellow(),
        name,
        report.issues.len(),
        if report.issues.len() == 1 { "" } else { "s" },
        if scripts > 0 { format!(" ({scripts} script line{} with unsupported calls)", if scripts == 1 { "" } else { "s" }) } else { String::new() }
    );
    for issue in &report.issues {
        println!("  {issue}");
    }
}

pub fn run_export(args: ExportArgs) -> Result<()> {
    let source = super::source::Source::open(&args.collection)?;
    let col = &source.collection;

    let output = match args.format {
        Format::Hitop => export::to_hitop(std::slice::from_ref(col))?,
        Format::Postman => export::to_postman(col)?,
    };

    match args.output {
        Some(path) => {
            std::fs::write(&path, &output)?;
            println!("{} Exported to {}", "✓".green(), path.display());
        }
        None => println!("{}", output),
    }

    Ok(())
}

fn timestamp_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
