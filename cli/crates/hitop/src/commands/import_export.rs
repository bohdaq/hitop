use anyhow::Result;
use clap::{Args, ValueEnum};
use colored::Colorize;
use std::path::PathBuf;

use hitop_core::{export, storage};

#[derive(ValueEnum, Clone)]
pub enum Format {
    Hitop,
    Postman,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Path to the JSON file
    file: PathBuf,

    /// Force format (auto-detected if omitted)
    #[arg(long)]
    format: Option<Format>,
}

#[derive(Args)]
pub struct ExportArgs {
    /// Collection name
    collection: String,

    /// Output format
    #[arg(long, default_value = "hitop")]
    format: Format,

    /// Output file (prints to stdout if omitted)
    #[arg(short = 'o', long)]
    output: Option<PathBuf>,
}

pub fn run_import(args: ImportArgs) -> Result<()> {
    let raw = std::fs::read_to_string(&args.file)?;
    let mut collections = storage::load_collections()?;

    let imported: Vec<hitop_core::model::Collection> = match args.format {
        Some(Format::Postman) => vec![export::from_postman(&raw)?],
        Some(Format::Hitop) | None => {
            // Auto-detect: Postman has "info.schema" key
            if raw.contains("schema.getpostman.com") {
                vec![export::from_postman(&raw)?]
            } else {
                export::from_hitop(&raw)?
            }
        }
    };

    let count = imported.len();
    for mut col in imported {
        // Regenerate ID to avoid collisions
        col.id = timestamp_id();
        // If name collides, suffix it
        if collections.iter().any(|c| c.name == col.name) {
            col.name = format!("{} (Imported)", col.name);
        }
        println!("{} Imported '{}'", "✓".green(), col.name.bold());
        collections.push(col);
    }

    storage::save_collections(&collections)?;
    println!("Imported {} collection(s).", count);
    Ok(())
}

pub fn run_export(args: ExportArgs) -> Result<()> {
    let collections = storage::load_collections()?;
    let col = collections
        .iter()
        .find(|c| c.name == args.collection)
        .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", args.collection))?;

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
