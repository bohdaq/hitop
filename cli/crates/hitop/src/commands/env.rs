use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;
use std::collections::HashMap;

use hitop_core::{model::Environment, storage};

#[derive(Subcommand)]
pub enum EnvCmd {
    /// List environments (the active one is marked)
    List,
    /// Create an environment
    Add { name: String },
    /// Delete an environment
    Delete { name: String },
    /// Show an environment's variables
    Show { name: String },
    /// Set a variable in an environment
    Set { name: String, key: String, value: String },
    /// Remove a variable from an environment
    Unset { name: String, key: String },
    /// Make an environment active for send, requests send and run
    Use { name: String },
    /// Turn off the active environment
    Off,
    /// Import environments: HITOP's export or a Postman environment file
    Import { file: String },
    /// Export all environments to a file
    Export { file: String },
}

pub fn run(cmd: EnvCmd) -> Result<()> {
    let mut envs = storage::load_environments()?;
    let find = |envs: &storage::Environments, name: &str| -> Result<usize> {
        envs.environments
            .iter()
            .position(|e| e.name == name)
            .ok_or_else(|| anyhow::anyhow!("environment '{}' not found", name))
    };

    match cmd {
        EnvCmd::List => {
            if envs.environments.is_empty() {
                println!("{}", "No environments. Create one with: hitop env add <name>".dimmed());
            }
            for e in &envs.environments {
                let active = envs.active.as_deref() == Some(e.id.as_str());
                println!("  {} {} {}", if active { "●".green() } else { "○".dimmed() }, e.name.bold(), format!("{} variables", e.variables.len()).dimmed());
            }
            return Ok(());
        }
        EnvCmd::Add { name } => {
            if envs.environments.iter().any(|e| e.name == name) {
                bail!("environment '{}' already exists", name);
            }
            envs.environments.push(Environment { id: new_id(), name: name.clone(), variables: HashMap::new(), ..Default::default() });
            println!("{} Created environment '{}'", "✓".green(), name.bold());
        }
        EnvCmd::Delete { name } => {
            let i = find(&envs, &name)?;
            let removed = envs.environments.remove(i);
            if envs.active.as_deref() == Some(removed.id.as_str()) {
                envs.active = None;
            }
            println!("{} Deleted environment '{}'", "✓".green(), name.bold());
        }
        EnvCmd::Show { name } => {
            let env = &envs.environments[find(&envs, &name)?];
            let mut keys: Vec<_> = env.variables.keys().collect();
            keys.sort();
            for k in keys {
                println!("  {} = {}", k.bold(), env.variables[k]);
            }
            return Ok(());
        }
        EnvCmd::Set { name, key, value } => {
            let i = find(&envs, &name)?;
            envs.environments[i].variables.insert(key.clone(), value.clone());
            println!("{} Set {}={} in '{}'", "✓".green(), key, value, name);
        }
        EnvCmd::Unset { name, key } => {
            let i = find(&envs, &name)?;
            if envs.environments[i].variables.remove(&key).is_none() {
                bail!("'{}' has no variable '{}'", name, key);
            }
            println!("{} Removed {} from '{}'", "✓".green(), key, name);
        }
        EnvCmd::Use { name } => {
            let i = find(&envs, &name)?;
            envs.active = Some(envs.environments[i].id.clone());
            println!("{} Using environment '{}'", "✓".green(), name.bold());
        }
        EnvCmd::Off => {
            envs.active = None;
            println!("{} No environment active", "✓".green());
        }
        EnvCmd::Import { file } => {
            let imported = parse(&std::fs::read_to_string(&file)?)?;
            let n = imported.len();
            envs.environments.extend(imported);
            println!("{} Imported {} environment{}", "✓".green(), n, if n == 1 { "" } else { "s" });
        }
        EnvCmd::Export { file } => {
            let out: Vec<_> = envs.environments.iter().map(|e| serde_json::json!({ "name": e.name, "variables": e.variables })).collect();
            std::fs::write(&file, serde_json::to_string_pretty(&out)?)?;
            println!("{} Exported {} environments to {}", "✓".green(), out.len(), file);
            return Ok(());
        }
    }
    storage::save_environments(&envs)
}

/// HITOP's export (`[{name, variables}]`) or Postman's (`{name, values: [{key, value, enabled}]}`), alone or in an array.
fn parse(json: &str) -> Result<Vec<Environment>> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let list = if let serde_json::Value::Array(a) = v { a } else { vec![v] };
    list.iter()
        .enumerate()
        .map(|(i, e)| {
            let mut variables = HashMap::new();
            if let Some(values) = e["values"].as_array() {
                for val in values {
                    if val["enabled"].as_bool() != Some(false) {
                        if let Some(k) = val["key"].as_str() {
                            variables.insert(k.to_string(), json_text(&val["value"]));
                        }
                    }
                }
            } else if let Some(obj) = e["variables"].as_object() {
                for (k, val) in obj {
                    variables.insert(k.clone(), json_text(val));
                }
            } else {
                bail!("item {} has no variables", i + 1);
            }
            let name = e["name"].as_str().map(str::to_string).unwrap_or_else(|| format!("Environment {}", i + 1));
            Ok(Environment { id: new_id(), name, variables, ..Default::default() })
        })
        .collect()
}

fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos())
}
