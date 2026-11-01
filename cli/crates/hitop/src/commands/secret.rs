//! `hitop secret set|get|rm|ls --env <env>`, `hitop secret check` and the
//! `--secret NAME=value` / `--no-secret-managers` flags. Values live in the
//! OS keychain (service `hitop`, account `<workspace>/<env>/<name>`) or in a
//! secret manager (`${secret:vault|aws|azure|op:...}` references), never in
//! collection files or JSON storage.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;

use hitop_core::{
    format,
    model::Environment,
    prepare::Scopes,
    secret_managers::{self, Resolver},
    secret_refs,
    secrets::{self, env_var, Lookup, NoStore, Origin, SecretStore, Secrets, StoreError},
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

    /// Don't resolve ${secret:vault|aws|azure|op:...} references (also
    /// HITOP_SECRET_MANAGERS=off); a request that uses one is an error
    #[arg(long)]
    pub no_secret_managers: bool,
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

/// The globals with their secret values filled in (env var
/// `HITOP_SECRET_GLOBAL_<NAME>`, `--secret`, keychain `<workspace>/global/<name>`,
/// prompt), and those secrets, for masking.
pub fn resolve_globals(workspace: &str, globals: &Environment, flags: &SecretFlags) -> Result<(std::collections::HashMap<String, String>, Secrets)> {
    let mut values = globals.variables.clone();
    for s in &globals.secrets {
        values.remove(s);
    }
    if globals.secrets.is_empty() {
        return Ok((values, Secrets::default()));
    }
    // Only the flags for the globals' own secrets: others belong to the environment.
    let pairs: Vec<(String, String)> = flags.pairs()?.into_iter().filter(|(k, _)| globals.secrets.contains(k)).collect();
    let store = store();
    let lookup = |k: &str| std::env::var(k).ok();
    let prompt: &dyn Fn(&str) -> Option<String> = &ask;
    let env = secrets::globals_lookup_env(globals);
    let resolved = secrets::resolve(&Lookup { workspace, environment: Some(&env), flags: &pairs, store: store.as_ref(), lookup_env: &lookup, prompt: interactive().then_some(prompt) })
        .map_err(|e| anyhow::anyhow!("{}", format!("{e:#}").replace(&format!("--env {}", secrets::GLOBALS_ENV), "--globals").replace("no value for secret", "no value for global secret")))?;
    for note in &resolved.notes {
        eprintln!("{} {note}", "note:".yellow());
    }
    resolved.secrets.apply(&mut values);
    Ok((values, resolved.secrets))
}

/// Resolves the `${secret:...}` references in `refs` (inner texts) into
/// `secrets` (origin "secret manager"), so they are filled in and masked like
/// every other secret. Every reference that fails is named in the error.
pub fn add_refs(secrets: &mut Secrets, refs: &BTreeSet<String>, flags: &SecretFlags) -> Result<()> {
    let refs: BTreeSet<String> = refs.iter().filter(|r| secrets.get(r).is_none()).cloned().collect();
    if refs.is_empty() {
        return Ok(());
    }
    if flags.no_secret_managers || !secret_managers::enabled() {
        let list: Vec<String> = refs.iter().map(|r| secret_refs::display(r)).collect();
        let why = if flags.no_secret_managers { "--no-secret-managers" } else { "HITOP_SECRET_MANAGERS=off" };
        bail!("secret manager references are turned off ({why}), but these are used: {}", list.join(", "));
    }
    let (values, errors) = Resolver::configured(store().as_ref()).map_err(|e| anyhow::anyhow!(e))?.resolve_all(&refs);
    if !errors.is_empty() {
        bail!("could not resolve {} secret reference{}:\n  {}", errors.len(), if errors.len() == 1 { "" } else { "s" }, errors.join("\n  "));
    }
    for (name, value) in values {
        secrets.insert(&name, value, Origin::SecretManager);
    }
    Ok(())
}

/// The references in variable values (collection, environment, globals, `--var`, data rows).
pub fn refs_in_vars<'a>(maps: impl IntoIterator<Item = &'a HashMap<String, String>>, out: &mut BTreeSet<String>) {
    for m in maps {
        secret_refs::find_in_variables(m, out);
    }
}

/// The references in strings (flags of `hitop send`, `ws`, `grpc`).
pub fn refs_in<'a>(texts: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in texts {
        secret_refs::find(t, &mut out);
    }
    out
}

/// Puts resolved references into the variables: as variables of their own
/// (`${secret:...}` in a request) and inside other variables' values.
pub fn fill_refs(secrets: &Secrets, scopes: &mut Scopes) {
    secrets.apply(&mut scopes.upper);
    secrets.expand_refs(&mut scopes.upper);
    secrets.expand_refs(&mut scopes.globals);
}

#[derive(Args)]
pub struct CheckArgs {
    /// Collection folder (or collection name); every collection in global storage when left out
    #[arg(short = 'c', long)]
    collection: Option<String>,

    /// Only this environment's variables (default: every environment)
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// Don't resolve, only list the references
    #[arg(long)]
    list: bool,
}

/// `hitop secret check`: every `${secret:...}` reference of a collection, its
/// environments and globals, where it is used, and whether it resolves.
/// Never prints a value.
fn check(args: CheckArgs) -> Result<()> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut note = |refs: BTreeSet<String>, place: String| {
        for r in refs {
            found.entry(r).or_default().insert(place.clone());
        }
    };
    let (collections, environments, globals) = match &args.collection {
        Some(c) => {
            let source = Source::open(c)?;
            let envs = match &source.environments {
                Some(e) => e.clone(),
                None => storage::load_environments()?.environments,
            };
            let globals = source.globals()?;
            (vec![source.collection], envs, globals)
        }
        None => (storage::load_collections()?, storage::load_environments()?.environments, storage::load_globals()?),
    };
    for col in &collections {
        for r in &col.requests {
            let mut refs = BTreeSet::new();
            secret_refs::find_in_request(r, &mut refs);
            note(refs, format!("request '{}'", r.name));
        }
        let mut refs = BTreeSet::new();
        secret_refs::find_in_variables(&col.variables, &mut refs);
        if let Some(auth) = &col.auth {
            secret_refs::find_in_json(&serde_json::to_value(auth)?, &mut refs);
        }
        note(refs, format!("collection '{}'", col.name));
    }
    let mut env_seen = false;
    for e in &environments {
        if args.env.as_deref().is_some_and(|n| n != e.name) {
            continue;
        }
        env_seen = true;
        let mut refs = BTreeSet::new();
        secret_refs::find_in_environment(e, &mut refs);
        note(refs, format!("environment '{}'", e.name));
    }
    if let (Some(name), false) = (&args.env, env_seen) {
        bail!("no environment '{name}'");
    }
    let mut refs = BTreeSet::new();
    secret_refs::find_in_environment(&globals, &mut refs);
    note(refs, "globals".into());

    if found.is_empty() {
        println!("{}", "No ${secret:...} references found.".dimmed());
        return Ok(());
    }
    let off = !args.list && !secret_managers::enabled();
    if off {
        eprintln!("{} HITOP_SECRET_MANAGERS=off: listing only", "note:".yellow());
    }
    if !args.list && !off {
        if let Some(path) = secret_managers::Settings::path().ok().filter(|p| p.exists()) {
            eprintln!("{} settings from {} fill in what the environment doesn't set", "note:".yellow(), path.display());
        }
    }
    let mut resolver = Resolver::configured(store().as_ref()).map_err(|e| anyhow::anyhow!(e))?;
    let mut failed = 0;
    for (r, places) in &found {
        let places: Vec<&str> = places.iter().map(String::as_str).collect();
        let shown = secret_refs::display(r);
        if args.list || off {
            println!("  {}  {}", shown.bold(), places.join(", ").dimmed());
            continue;
        }
        match resolver.resolve(r) {
            Ok(_) => println!("  {} {}  {}", "✓".green(), shown.bold(), places.join(", ").dimmed()),
            Err(e) => {
                failed += 1;
                let why = e.strip_prefix(&shown).unwrap_or(&e).trim_start();
                println!("  {} {}  {}\n      {}", "✗".red(), shown.bold(), places.join(", ").dimmed(), why.red());
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} secret references did not resolve", found.len());
    }
    if !args.list && !off {
        println!("{} {} secret reference{} resolved", "✓".green(), found.len(), if found.len() == 1 { "" } else { "s" });
    }
    Ok(())
}

#[derive(Args)]
#[command(group(clap::ArgGroup::new("scope").required(true).args(["env", "globals"])))]
pub struct Target {
    /// Environment the secret belongs to
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// A secret of the globals instead of an environment
    /// (env var HITOP_SECRET_GLOBAL_<NAME>)
    #[arg(long)]
    globals: bool,

    /// Collection folder (or collection name) whose environment (or
    /// globals.yaml) it is; global storage when left out
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
    /// List the ${secret:vault|aws|azure|op:...} references of a collection,
    /// its environments and globals, and check that each one resolves (never
    /// prints a value; exit 1 if one doesn't)
    Check(CheckArgs),
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
    /// The globals: `env` is them under the name [`secrets::GLOBALS_ENV`].
    globals: Option<super::globals::Place>,
}

fn place(t: &Target) -> Result<Place> {
    if t.globals {
        let g = super::globals::Place::of(t.collection.as_deref())?;
        let env = secrets::globals_lookup_env(&g.load()?);
        return Ok(Place { workspace: g.workspace.clone(), env, folder: g.folder.clone(), globals: Some(g) });
    }
    let name = t.env.as_deref().context("--env or --globals")?;
    if let Some(c) = &t.collection {
        let source = Source::open(c)?;
        let env = source.environment(Some(name))?.context("no environment")?;
        return Ok(Place { workspace: source.workspace(), env, folder: source.folder().map(|p| p.to_path_buf()), globals: None });
    }
    let env = storage::load_environments()?.pick(Some(name))?.cloned().context("no environment")?;
    Ok(Place { workspace: secrets::GLOBAL_WORKSPACE.into(), env, folder: None, globals: None })
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
    if let SecretCmd::Check(args) = cmd {
        return check(args);
    }
    let store = store();
    match cmd {
        SecretCmd::Check(_) => unreachable!(),
        SecretCmd::Set { name, value, target } => {
            let p = place(&target)?;
            let env = p.env.name.as_str();
            let value = match value {
                Some(v) => v,
                None => read_value(&format!("{name} ({env})"))?,
            };
            store.set(&secrets::account(&p.workspace, env, &name), &value).map_err(|e| keychain_error(e, env, &name))?;
            if let Some(g) = &p.globals {
                let mut globals = g.load()?;
                globals.variables.remove(&name);
                if !globals.secrets.contains(&name) {
                    globals.secrets.push(name.clone());
                    globals.secrets.sort();
                }
                g.save(&globals)?;
                println!("{} Stored global secret {} in the OS keychain", "✓".green(), name.bold());
                return Ok(());
            }
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
                let hint = if p.globals.is_some() { "The globals have no secrets. Add one with: hitop secret set <name> --globals".to_string() } else { format!("'{env}' has no secrets. Add one with: hitop secret set <name> --env {env}") };
                println!("{}", hint.dimmed());
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
