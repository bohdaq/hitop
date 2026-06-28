use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use std::collections::HashMap;

use hitop_core::storage;

#[derive(Subcommand)]
pub enum CollectionsCmd {
    /// List all collections
    List,
    /// Add a new collection
    Add {
        name: String,
    },
    /// Delete a collection
    Delete {
        name: String,
    },
    /// Rename a collection
    Rename {
        name: String,
        new_name: String,
    },
    /// Duplicate a collection
    Duplicate {
        name: String,
        /// New name (defaults to "<name> (Copy)")
        #[arg(long)]
        as_name: Option<String>,
    },
}

pub fn run(cmd: CollectionsCmd) -> Result<()> {
    let mut collections = storage::load_collections()?;

    match cmd {
        CollectionsCmd::List => {
            if collections.is_empty() {
                println!("{}", "No collections yet. Use `hitop collections add <name>`.".dimmed());
                return Ok(());
            }
            for col in &collections {
                println!(
                    "  {} {} ({} requests)",
                    "•".cyan(),
                    col.name.bold(),
                    col.requests.len()
                );
            }
        }

        CollectionsCmd::Add { name } => {
            if collections.iter().any(|c| c.name == name) {
                bail!("collection '{}' already exists", name);
            }
            let id = timestamp_id();
            collections.push(hitop_core::model::Collection {
                id,
                name: name.clone(),
                requests: vec![],
                variables: HashMap::new(),
            });
            storage::save_collections(&collections)?;
            println!("{} Created collection '{}'", "✓".green(), name.bold());
        }

        CollectionsCmd::Delete { name } => {
            let len_before = collections.len();
            collections.retain(|c| c.name != name);
            if collections.len() == len_before {
                bail!("collection '{}' not found", name);
            }
            storage::save_collections(&collections)?;
            println!("{} Deleted collection '{}'", "✓".green(), name.bold());
        }

        CollectionsCmd::Rename { name, new_name } => {
            let col = collections
                .iter_mut()
                .find(|c| c.name == name)
                .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", name))?;
            col.name = new_name.clone();
            storage::save_collections(&collections)?;
            println!("{} Renamed '{}' → '{}'", "✓".green(), name.bold(), new_name.bold());
        }

        CollectionsCmd::Duplicate { name, as_name } => {
            let col = collections
                .iter()
                .find(|c| c.name == name)
                .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", name))?
                .clone();

            let new_name = as_name.unwrap_or_else(|| format!("{} (Copy)", col.name));
            let new_id = timestamp_id();

            let mut new_col = col;
            new_col.id = new_id;
            new_col.name = new_name.clone();
            // Regenerate request IDs
            for r in &mut new_col.requests {
                r.id = timestamp_id();
            }

            collections.push(new_col);
            storage::save_collections(&collections)?;
            println!("{} Duplicated as '{}'", "✓".green(), new_name.bold());
        }
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
