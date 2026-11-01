//! `hitop import openapi <file|url>` and `hitop openapi sync <file|url> <folder>`.
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use hitop_core::{format, http, openapi, postman::ImportReport};

#[derive(Args)]
pub struct ImportOpenapiArgs {
    /// The spec: a file (YAML or JSON) or an http(s) URL
    source: String,

    /// Write a collection folder here instead of global storage
    #[arg(long, value_name = "DIR")]
    to: Option<PathBuf>,

    /// With --to: replace the collection in a folder that already has one
    #[arg(long)]
    force: bool,

    /// Every request at the top level instead of a folder per tag
    #[arg(long)]
    flat: bool,
}

#[derive(Subcommand)]
pub enum OpenapiCmd {
    /// Import a spec as a collection (same as `hitop import openapi`)
    Import(ImportOpenapiArgs),
    /// Update a collection folder imported from a spec: new operations are
    /// added, changed ones updated where you didn't edit them, removed ones
    /// listed (and deleted only with --prune)
    Sync(SyncArgs),
}

#[derive(Args)]
pub struct SyncArgs {
    /// The newer spec: a file (YAML or JSON) or an http(s) URL
    source: String,

    /// The collection folder imported from the spec before
    folder: PathBuf,

    /// Show what would change; write nothing
    #[arg(long)]
    dry_run: bool,

    /// Delete the requests of operations the spec no longer has
    #[arg(long)]
    prune: bool,
}

pub fn run(cmd: OpenapiCmd) -> Result<()> {
    match cmd {
        OpenapiCmd::Import(args) => run_import(args),
        OpenapiCmd::Sync(args) => run_sync(args),
    }
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// The spec's text, and the source to record (URLs only: a file path means
/// nothing on another machine).
pub fn load(source: &str) -> Result<(String, Option<String>)> {
    if is_url(source) {
        let headers = [("Accept".to_string(), "application/yaml, application/json;q=0.9, */*;q=0.8".to_string())];
        let opts = http::Options { timeout: Some(Duration::from_secs(60)), jar: None };
        let resp = http::send_with("GET", source, &headers, vec![], opts).with_context(|| format!("fetching {source}"))?;
        if resp.status >= 400 {
            bail!("fetching {source}: the server answered {} {}", resp.status, resp.status_text);
        }
        return Ok((String::from_utf8_lossy(&resp.body).into_owned(), Some(source.to_string())));
    }
    let text = std::fs::read_to_string(source).with_context(|| format!("reading {source}"))?;
    Ok((text, None))
}

pub fn run_import(args: ImportOpenapiArgs) -> Result<()> {
    let (text, source) = load(&args.source)?;
    import_text(&text, source, args.to.as_deref(), args.force, args.flat)
}

/// Imports a spec already read (also `hitop import <spec>`).
pub fn import_text(text: &str, source: Option<String>, to: Option<&Path>, force: bool, flat: bool) -> Result<()> {
    let opts = openapi::Options { flat, source, inline_auth: false };
    let (collection, report) = openapi::import(text, &opts)?;
    let mut list = vec![(collection, ImportReport::default())];
    match to {
        Some(dir) => {
            super::import_export::import_to_folder(dir, &mut list, &mut [], force)?;
        }
        None => {
            super::import_export::import_to_storage(&mut list, &mut [])?;
        }
    }
    print_report(&report);
    Ok(())
}

fn plural(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

pub fn print_report(report: &openapi::OpenApiReport) {
    let m = &report.mapped;
    println!(
        "  OpenAPI {}: {} → {} in {}, {}",
        m.openapi,
        plural(m.operations, "operation"),
        plural(m.requests, "request"),
        plural(m.folders, "folder"),
        plural(m.examples, "saved example")
    );
    println!("  variables: {}", m.variables.join(", "));
    if !m.auth.is_empty() {
        println!("  auth: {}", m.auth.join("; "));
    }
    if !report.skipped.is_empty() {
        println!("{} {} not imported:", "!".yellow(), plural(report.skipped.len(), "thing"));
        for i in &report.skipped {
            println!("  {i}");
        }
    }
    if !report.warnings.is_empty() {
        println!("{} {} to check:", "!".yellow(), plural(report.warnings.len(), "thing"));
        for i in &report.warnings {
            println!("  {i}");
        }
    }
}

fn op_line(c: &openapi::OpChange) -> String {
    match &c.operation_id {
        Some(id) => format!("{} ({id})", c.operation),
        None => c.operation.clone(),
    }
}

pub fn run_sync(args: SyncArgs) -> Result<()> {
    let dir = &args.folder;
    let folder = format::read_folder(dir)?;
    let (text, source) = load(&args.source)?;
    let opts = openapi::Options { source, ..Default::default() };
    let synced = openapi::sync(&folder.collection, &text, &opts, args.prune)?;
    let d = &synced.diff;
    println!("Sync '{}' with {}{}", folder.collection.name.bold(), args.source, if args.dry_run { " (dry run: nothing is written)" } else { "" });
    for a in &d.added {
        println!("  {} {}  {} → requests/{}.yaml", "+".green(), op_line(a), a.name, a.file);
    }
    for c in &d.changed {
        let mut parts = vec![];
        if !c.updated.is_empty() {
            parts.push(format!("updated {}", c.updated.join(", ")));
        }
        if !c.kept.is_empty() {
            parts.push(format!("kept your {} (the spec changed it too)", c.kept.join(", ")));
        }
        if parts.is_empty() {
            parts.push("operation renamed".into());
        }
        println!("  {} {}  {}: {}", "~".yellow(), op_line(c), c.name, parts.join("; "));
    }
    for r in &d.removed {
        let what = if d.pruned { "removed from the spec: deleted".to_string() } else { "removed from the spec: kept (--prune deletes it)".to_string() };
        println!("  {} {}  {}: {what}", "-".red(), op_line(r), r.name);
    }
    let cc = &d.collection;
    if !cc.updated.is_empty() {
        println!("  {} collection: updated {}", "~".yellow(), cc.updated.join(", "));
    }
    if !cc.kept.is_empty() {
        println!("  {} collection: kept your {} (the spec changed it too)", "~".yellow(), cc.kept.join(", "));
    }
    if !cc.variables_added.is_empty() {
        println!("  {} collection: new variables {}", "+".green(), cc.variables_added.join(", "));
    }
    println!(
        "  {} added, {} changed, {} removed, {} unchanged",
        d.added.len(),
        d.changed.len(),
        d.removed.len(),
        d.unchanged
    );
    if !synced.report.skipped.is_empty() {
        println!("{} {} in the spec not imported (see `hitop import openapi`)", "!".yellow(), plural(synced.report.skipped.len(), "thing"));
    }
    if args.dry_run {
        return Ok(());
    }
    if d.is_empty() && d.changed.is_empty() {
        println!("{} {} is up to date", "✓".green(), dir.display());
        return Ok(());
    }
    format::write_collection(dir, &synced.collection)?;
    println!("{} Updated {}", "✓".green(), dir.display());
    Ok(())
}
