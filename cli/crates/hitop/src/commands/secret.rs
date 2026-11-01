//! `hitop secret set|get|rm|ls --env <env>` and the `--secret NAME=value`
//! flag. Values live in the OS keychain (service `hitop`, account
//! `<workspace>/<env>/<name>`), never in collection files or JSON storage.
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;

use hitop_core::{
    format,
    model::Environment,
    secrets::{self, env_var, Lookup, NoStore, SecretStore, Secrets, StoreError},
    storage,
};

use super::source::Source;

#[derive(Args, Default)]
pub struct SecretFlags {
    /// Secret variable for this run, as NAME=VALUE (repeatable). Overrides
    /// the keychain; masked as •••• in output and history. Prefer the
    /// HITOP_SECRET_<ENV>_<NAME> env var where shell history is kept.
    #[arg(long = "secret", value_name = "NAME=VALUE")]
    pub secret: Vec<String>,
}

impl SecretFlags {
    pub fn pairs(&self) -> Result<Vec<(String, String)>> {
        self.secret
            .iter()
            .map(|s| match s.split_once('=') {
                Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
                _ => bail!("--secret takes NAME=VALUE"),
            })
            .collect()
    }
}

/// The OS keychain, unless `HITOP_KEYCHAIN=off` (CI and tests that must not
/// touch it).
pub fn store() -> Box<dyn SecretStore> {
    if std::env::var("HITOP_KEYCHAIN").is_ok_and(|v| v == "off") {
        return Box::new(NoStore("turned off by HITOP_KEYCHAIN=off".into()));
    }
    secrets::os_store()
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn ask(label: &str) -> Option<String> {
    rpassword::prompt_password(format!("Value for secret {label}: ")).ok()
}

/// Resolves the environment's secrets and the `--secret` flags (env var,
/// flag, keychain, then a prompt on a terminal). Notes go to stderr.
pub fn resolve(workspace: &str, environment: Option<&Environment>, flags: &SecretFlags) -> Result<Secrets> {
    let pairs = flags.pairs()?;
    let needs_any = !pairs.is_empty() || environment.is_some_and(|e| !e.secrets.is_empty());
    if !needs_any {
        return Ok(Secrets::default());
    }
    let store = store();
    let lookup = |k: &str| std::env::var(k).ok();
    let prompt: &dyn Fn(&str) -> Option<String> = &ask;
    let resolved = secrets::resolve(&Lookup {
        workspace,
        environment,
        flags: &pairs,
        store: store.as_ref(),
        lookup_env: &lookup,
        prompt: interactive().then_some(prompt),
    })?;
    for note in &resolved.notes {
        eprintln!("{} {note}", "note:".yellow());
    }
    Ok(resolved.secrets)
}

#[derive(Args)]
pub struct Target {
    /// Environment the secret belongs to
    #[arg(short = 'e', long = "env")]
    env: String,

    /// Collection folder (or collection name) whose environment it is;
    /// global environments when left out
    #[arg(short = 'c', long)]
    collection: Option<String>,
}

#[derive(Subcommand)]
pub enum SecretCmd {
    /// Store a secret's value in the OS keychain and mark the variable secret
    /// in the environment. Without VALUE it is read from a prompt, or from
    /// stdin when that isn't a terminal.
    Set {
        name: String,
        value: Option<String>,
        #[command(flatten)]
        target: Target,
    },
    /// Print a secret's value from the OS keychain
    Get {
        name: String,
        #[command(flatten)]
        target: Target,
    },
    /// Remove a secret's value from the OS keychain (the variable stays
    /// marked secret)
    Rm {
        name: String,
        #[command(flatten)]
        target: Target,
    },
    /// List the environment's secrets and where each value comes from (never
    /// the values)
    Ls {
        #[command(flatten)]
        target: Target,
    },
}

struct Place {
    workspace: String,
    env: Environment,
    folder: Option<PathBuf>,
}

fn place(t: &Target) -> Result<Place> {
    if let Some(c) = &t.collection {
        let source = Source::open(c)?;
        let env = source.environment(Some(&t.env))?.context("no environment")?;
        return Ok(Place { workspace: source.workspace(), env, folder: source.folder().map(|p| p.to_path_buf()) });
    }
    let env = storage::load_environments()?.pick(Some(&t.env))?.cloned().context("no environment")?;
    Ok(Place { workspace: secrets::GLOBAL_WORKSPACE.into(), env, folder: None })
}

fn keychain_error(e: StoreError, env: &str, name: &str) -> anyhow::Error {
    match e {
        StoreError::Unavailable(_) => anyhow::anyhow!(
            "{e}\nOn CI and headless machines give the value as the {} env var or with --secret {name}=...",
            env_var(env, name)
        ),
        other => other.into(),
    }
}

fn read_value(label: &str) -> Result<String> {
    if interactive() {
        return ask(label).context("no value entered");
    }
    let mut v = String::new();
    std::io::stdin().read_to_string(&mut v)?;
    let v = v.strip_suffix('\n').map(|s| s.strip_suffix('\r').unwrap_or(s)).unwrap_or(&v).to_string();
    if v.is_empty() {
        bail!("no value given: pass it as an argument, type it at the prompt, or pipe it on stdin");
    }
    Ok(v)
}

pub fn run(cmd: SecretCmd) -> Result<()> {
    let store = store();
    match cmd {
        SecretCmd::Set { name, value, target } => {
            let p = place(&target)?;
            let env = p.env.name.as_str();
            let value = match value {
                Some(v) => v,
                None => read_value(&format!("{name} ({env})"))?,
            };
            store.set(&secrets::account(&p.workspace, env, &name), &value).map_err(|e| keychain_error(e, env, &name))?;
            match &p.folder {
                Some(dir) => {
                    format::mark_secret(dir, env, &name)?;
                }
                None => {
                    let mut envs = storage::load_environments()?;
                    if let Some(e) = envs.environments.iter_mut().find(|e| e.name == env) {
                        e.variables.remove(&name);
                        if !e.secrets.contains(&name) {
                            e.secrets.push(name.clone());
                            e.secrets.sort();
                        }
                    }
                    storage::save_environments(&envs)?;
                }
            }
            println!("{} Stored secret {} for '{}' in the OS keychain", "✓".green(), name.bold(), env);
        }
        SecretCmd::Get { name, target } => {
            let p = place(&target)?;
            let env = p.env.name.as_str();
            match store.get(&secrets::account(&p.workspace, env, &name)).map_err(|e| keychain_error(e, env, &name))? {
                Some(v) => println!("{v}"),
                None => bail!("the OS keychain has no value for {name} in '{env}'"),
            }
        }
        SecretCmd::Rm { name, target } => {
            let p = place(&target)?;
            let env = p.env.name.as_str();
            if store.delete(&secrets::account(&p.workspace, env, &name)).map_err(|e| keychain_error(e, env, &name))? {
                println!("{} Removed {} for '{}' from the OS keychain", "✓".green(), name.bold(), env);
            } else {
                println!("{}", format!("The OS keychain had no value for {name} in '{env}'").dimmed());
            }
        }
        SecretCmd::Ls { target } => {
            let p = place(&target)?;
            let env = p.env.name.as_str();
            if p.env.secrets.is_empty() {
                println!("{}", format!("'{env}' has no secrets. Add one with: hitop secret set <name> --env {env}").dimmed());
                return Ok(());
            }
            let mut down: Option<StoreError> = None;
            for name in &p.env.secrets {
                let var = env_var(env, name);
                let source = if std::env::var_os(&var).is_some() {
                    format!("env var {var}").green().to_string()
                } else if down.is_some() {
                    "unknown (no keychain)".yellow().to_string()
                } else {
                    match store.get(&secrets::account(&p.workspace, env, name)) {
                        Ok(Some(_)) => "keychain".green().to_string(),
                        Ok(None) => "missing".red().to_string(),
                        Err(e) => {
                            down = Some(e);
                            "unknown (no keychain)".yellow().to_string()
                        }
                    }
                };
                println!("  {} {}", name.bold(), source);
            }
            if let Some(e) = down {
                eprintln!("{} {e}", "note:".yellow());
            }
        }
    }
    Ok(())
}
