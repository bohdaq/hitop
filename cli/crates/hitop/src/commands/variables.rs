use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;

use hitop_core::storage;

#[derive(Subcommand)]
pub enum VariablesCmd {
    /// List variables for a collection
    List {
        collection: String,
    },
    /// Set a variable
    Set {
        collection: String,
        key: String,
        value: String,
    },
    /// Delete a variable
    Delete {
        collection: String,
        key: String,
    },
}

pub fn run(cmd: VariablesCmd) -> Result<()> {
    let mut collections = storage::load_collections()?;

    match cmd {
        VariablesCmd::List { collection } => {
            let col = collections
                .iter()
                .find(|c| c.name == collection)
                .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", collection))?;

            if col.variables.is_empty() {
                println!("{}", "No variables defined.".dimmed());
                return Ok(());
            }

            let mut keys: Vec<&String> = col.variables.keys().collect();
            keys.sort();
            for k in keys {
                println!("  {} {} = {}", "•".cyan(), k.bold(), col.variables[k]);
            }
        }

        VariablesCmd::Set { collection, key, value } => {
            let col = collections
                .iter_mut()
                .find(|c| c.name == collection)
                .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", collection))?;

            col.variables.insert(key.clone(), value.clone());
            storage::save_collections(&collections)?;
            println!("{} Set {}={}", "✓".green(), key.bold(), value);
        }

        VariablesCmd::Delete { collection, key } => {
            let col = collections
                .iter_mut()
                .find(|c| c.name == collection)
                .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", collection))?;

            if col.variables.remove(&key).is_none() {
                anyhow::bail!("variable '{}' not found", key);
            }
            storage::save_collections(&collections)?;
            println!("{} Deleted variable '{}'", "✓".green(), key.bold());
        }
    }

    Ok(())
}
