use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;

use super::source::Source;

#[derive(Subcommand)]
pub enum VariablesCmd {
    /// List variables for a collection
    List {
        /// Collection name or folder path
        collection: String,
    },
    /// Set a variable
    Set {
        /// Collection name or folder path
        collection: String,
        key: String,
        value: String,
    },
    /// Delete a variable
    Delete {
        /// Collection name or folder path
        collection: String,
        key: String,
    },
}

pub fn run(cmd: VariablesCmd) -> Result<()> {

    match cmd {
        VariablesCmd::List { collection } => {
            let source = Source::open(&collection)?;
            let col = &source.collection;

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
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;

            col.variables.insert(key.clone(), value.clone());
            source.save()?;
            println!("{} Set {}={}", "✓".green(), key.bold(), value);
        }

        VariablesCmd::Delete { collection, key } => {
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;

            if col.variables.remove(&key).is_none() {
                anyhow::bail!("variable '{}' not found", key);
            }
            source.save()?;
            println!("{} Deleted variable '{}'", "✓".green(), key.bold());
        }
    }

    Ok(())
}
