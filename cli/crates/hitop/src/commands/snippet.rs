//! `hitop snippet <collection> <request> --lang <lang>`: the request as code
//! (hitop-core's generator, the same one the web app runs through hitop-wasm).
use anyhow::{anyhow, Result};
use clap::Args;

use hitop_core::snippet::{generate, Lang};

use super::secret::SecretFlags;
use super::source::Source;

#[derive(Args)]
pub struct SnippetArgs {
    /// Collection name or folder path
    pub collection: String,
    /// Request name
    pub request: String,
    /// curl, fetch (JavaScript), python (requests), go (net/http), java (HttpClient) or rust (reqwest)
    #[arg(short = 'l', long = "lang", default_value = "curl", value_parser = parse_lang)]
    pub lang: Lang,
    /// Environment to fill variables from instead of the active one
    #[arg(short = 'e', long = "env")]
    pub env: Option<String>,
    /// Write secret values into the code (env var, --secret, keychain, prompt) instead of {{NAME}} placeholders
    #[arg(long)]
    pub show_secrets: bool,
    #[command(flatten)]
    pub secrets: SecretFlags,
}

fn parse_lang(s: &str) -> Result<Lang, String> {
    Lang::parse(s).ok_or_else(|| format!("unknown language '{s}': use {}", Lang::ALL.map(|l| l.id()).join(", ")))
}

pub fn run(args: SnippetArgs) -> Result<()> {
    let source = Source::open(&args.collection)?;
    let col = &source.collection;
    let req = col
        .requests
        .iter()
        .find(|r| r.name == args.request)
        .ok_or_else(|| anyhow!("request '{}' not found in '{}'", args.request, col.name))?;
    let environment = source.environment(args.env.as_deref())?;
    let globals = source.globals()?;

    // Globals below collection variables below the environment, as `requests send` fills them.
    let mut vars = globals.variables.clone();
    vars.extend(col.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
    if let Some(env) = &environment {
        vars.extend(env.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let mut secret_names: Vec<String> = environment.iter().flat_map(|e| e.secrets.iter().cloned()).chain(globals.secrets.iter().cloned()).collect();
    secret_names.sort();
    secret_names.dedup();
    // ${secret:vault|aws|azure|op:...} references: resolved only with --show-secrets.
    let mut refs = std::collections::BTreeSet::new();
    hitop_core::secret_refs::find_in_request(req, &mut refs);
    if let Some(auth) = &col.auth {
        hitop_core::secret_refs::find_in_json(&serde_json::to_value(auth)?, &mut refs);
    }
    hitop_core::secret_refs::find_in_variables(&vars, &mut refs);
    if args.show_secrets {
        let workspace = source.workspace();
        let (_, global_secrets) = super::secret::resolve_globals(&workspace, &globals, &args.secrets)?;
        global_secrets.apply(&mut vars);
        let mut secrets = super::secret::resolve(&workspace, environment.as_ref(), &args.secrets)?;
        super::secret::add_refs(&mut secrets, &refs, &args.secrets)?;
        secrets.apply(&mut vars);
        secrets.expand_refs(&mut vars);
        secret_names.clear();
    } else {
        // Never the stored value, not even a stale one in a JSON environment.
        for s in &secret_names {
            vars.remove(s);
        }
        // A reference inside a variable's value becomes the placeholder too.
        let placeholder = |r: &str| Some(hitop_core::snippet::placeholder(r));
        for v in vars.values_mut() {
            *v = hitop_core::secret_refs::expand(v, &placeholder);
        }
        secret_names.extend(refs);
    }
    let snippet = generate(args.lang, req, col.auth.as_ref(), &vars, &secret_names)?;
    print!("{}", snippet.code);
    let (secret, missing): (Vec<&String>, Vec<&String>) = snippet.placeholders.iter().partition(|p| secret_names.contains(p));
    if !secret.is_empty() {
        eprintln!("note: secrets written as placeholders: {} (--show-secrets writes their values)", list(&secret));
    }
    if !missing.is_empty() {
        eprintln!("note: variables with no value written as placeholders: {}", list(&missing));
    }
    Ok(())
}

fn list(names: &[&String]) -> String {
    names.iter().map(|n| hitop_core::snippet::placeholder(n)).collect::<Vec<_>>().join(", ")
}
