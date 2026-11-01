use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;

use hitop_core::{model::{Auth, Header, Request}, params::append_query, storage};

use super::flags::{query_pair, AuthFlags};
use super::source::Source;

#[derive(Subcommand)]
pub enum RequestsCmd {
    /// List requests in a collection
    List {
        /// Collection name or folder path
        collection: String,
    },
    /// Add a request to a collection
    Add {
        /// Collection name or folder path
        collection: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        url: String,
        #[arg(long, short = 'X', default_value = "GET")]
        method: String,
        /// Headers in "Name: Value" format (repeatable)
        #[arg(short = 'H', long = "header")]
        headers: Vec<String>,
        /// Request body
        #[arg(short = 'b', long)]
        body: Option<String>,
        /// Query parameter appended to the URL, as key=value (repeatable)
        #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
        query: Vec<String>,
        #[command(flatten)]
        auth: AuthFlags,
    },
    /// Set or clear a request's auth
    Auth {
        /// Collection name or folder path
        collection: String,
        request: String,
        #[command(flatten)]
        auth: AuthFlags,
        /// Remove the request's auth
        #[arg(long, conflicts_with_all = ["bearer", "basic", "api_key"])]
        none: bool,
    },
    /// Show a request's details
    Show {
        /// Collection name or folder path
        collection: String,
        request: String,
    },
    /// Delete a request
    Delete {
        /// Collection name or folder path
        collection: String,
        request: String,
    },
    /// Rename a request
    Rename {
        /// Collection name or folder path
        collection: String,
        request: String,
        new_name: String,
    },
    /// Load and immediately send a request from a collection
    Send {
        /// Collection name or folder path
        collection: String,
        request: String,
        /// Show response headers
        #[arg(long)]
        show_headers: bool,
        /// Disable syntax highlighting
        #[arg(long)]
        raw: bool,
        /// Environment to use instead of the active one
        #[arg(short = 'e', long = "env")]
        env: Option<String>,

        #[command(flatten)]
        secrets: super::secret::SecretFlags,

        #[command(flatten)]
        cookies: super::cookies::CookieFlags,
    },
}

pub fn run(cmd: RequestsCmd) -> Result<()> {

    match cmd {
        RequestsCmd::List { collection } => {
            let source = Source::open(&collection)?;
            let col = &source.collection;
            if col.requests.is_empty() {
                println!("{}", "No requests in this collection.".dimmed());
                return Ok(());
            }
            for r in &col.requests {
                println!(
                    "  {} {} {}  {}",
                    "•".cyan(),
                    r.method.bold(),
                    r.name.bold(),
                    r.url.dimmed()
                );
            }
        }

        RequestsCmd::Add { collection, name, url, method, headers, body, query, auth } => {
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;
            let headers: Vec<Header> = headers
                .iter()
                .filter_map(|h| {
                    let (k, v) = h.split_once(':')?;
                    Some(Header::new(k.trim(), v.trim()))
                })
                .collect();
            let req = Request {
                id: timestamp_id(),
                name: name.clone(),
                url: append_query(&url, &query.iter().map(|q| query_pair(q)).collect::<Vec<_>>()),
                method,
                headers: if headers.is_empty() { vec![Header::new("", "")] } else { headers },
                body: body.unwrap_or_default(),
                pre_request_script: String::new(),
                post_request_script: String::new(),
                auth: auth.auth()?,
                ..Default::default()
            };
            col.requests.push(req);
            source.save()?;
            println!("{} Added request '{}' to '{}'", "✓".green(), name.bold(), collection.bold());
        }

        RequestsCmd::Auth { collection, request, auth, none } => {
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;
            let req = col
                .requests
                .iter_mut()
                .find(|r| r.name == request)
                .ok_or_else(|| anyhow::anyhow!("request '{}' not found", request))?;
            req.auth = if none { None } else {
                match auth.auth()? {
                    Some(a) => Some(a),
                    None => bail!("give --bearer, --basic, --api-key or --none"),
                }
            };
            source.save()?;
            println!("{} Updated auth for '{}'", "✓".green(), request.bold());
        }

        RequestsCmd::Show { collection, request } => {
            let source = Source::open(&collection)?;
            let col = &source.collection;
            let req = find_req(col, &request)?;
            println!("{}: {}", "Name".bold(), req.name);
            println!("{}: {}", "Method".bold(), req.method);
            println!("{}: {}", "URL".bold(), req.url);
            let headers: Vec<_> = req.headers.iter().filter(|h| !h.name.is_empty()).collect();
            for h in headers {
                println!("{}: {}: {}", "Header".bold(), h.name, h.value);
            }
            if let Some(auth) = &req.auth {
                println!("{}: {}", "Auth".bold(), describe_auth(auth));
            }
            if !req.body.is_empty() {
                println!("{}: {}", "Body".bold(), req.body);
            }
            if !req.pre_request_script.is_empty() {
                println!("{}: {}", "Pre-request script".bold(), req.pre_request_script);
            }
            if !req.post_request_script.is_empty() {
                println!("{}: {}", "Post-request script".bold(), req.post_request_script);
            }
        }

        RequestsCmd::Delete { collection, request } => {
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;
            let len_before = col.requests.len();
            col.requests.retain(|r| r.name != request);
            if col.requests.len() == len_before {
                bail!("request '{}' not found in '{}'", request, collection);
            }
            source.save()?;
            println!("{} Deleted request '{}'", "✓".green(), request.bold());
        }

        RequestsCmd::Rename { collection, request, new_name } => {
            let mut source = Source::open(&collection)?;
            let col = &mut source.collection;
            let req = col
                .requests
                .iter_mut()
                .find(|r| r.name == request)
                .ok_or_else(|| anyhow::anyhow!("request '{}' not found", request))?;
            req.name = new_name.clone();
            source.save()?;
            println!("{} Renamed '{}' → '{}'", "✓".green(), request.bold(), new_name.bold());
        }

        RequestsCmd::Send { collection, request, show_headers, raw, env, secrets, cookies } => {
            use hitop_core::{http, prepare::{prepare_in, variables, Draft, Scopes}, scripting::{run_post_request_script_with, PostRequestCtx}};
            use crate::output;

            let source = Source::open(&collection)?;
            let col = &source.collection;
            let req = find_req(col, &request)?.clone();
            let environment = source.environment(env.as_deref())?;
            let secret_flags = secrets;
            let secrets = super::secret::resolve(&source.workspace(), environment.as_ref(), &secret_flags)?;
            let mut vars = variables(&col.variables, environment.as_ref());
            secrets.apply(&mut vars);
            let (globals, global_secrets) = super::secret::resolve_globals(&source.workspace(), &source.globals()?, &secret_flags)?;
            let mut scopes = Scopes { globals, upper: vars };
            let masker = secrets.masker_with(&global_secrets);
            let masked = |e: anyhow::Error| anyhow::anyhow!(masker.mask(&format!("{e:#}")));
            let context_key = source.context_key();

            // Load context
            let contexts = storage::load_contexts()?;
            let context: std::collections::HashMap<String, serde_json::Value> = contexts
                .get(&context_key)
                .and_then(|v| v.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            let draft = Draft::of(&req, col.auth.as_ref());
            let pre = prepare_in(&draft, &scopes, &context).map_err(masked)?;
            scopes.apply(&pre.variables, &pre.globals);
            let vars = scopes.merged();
            let (headers, body) = hitop_core::form::outgoing(&req, &pre.headers, &pre.body, &vars).map_err(masked)?;
            let (sent_headers, sent_body) = (headers.clone(), String::from_utf8(body.clone()).unwrap_or_default());
            let workspace = source.workspace();
            let mut jar = super::cookies::open(&workspace, cookies)?;
            let opts = http::Options { timeout: None, jar: jar.as_mut() };
            let mut result = http::execute_opts(&req.method, &pre.url, &headers, body, opts).map_err(masked)?;
            super::cookies::save(&workspace, jar.as_ref())?;

            let mut post = run_post_request_script_with(
                &req.post_request_script,
                &PostRequestCtx {
                    response_body: &result.body,
                    response_headers: &result.headers,
                    status_code: result.status,
                    duration_ms: result.duration_ms,
                    variables: &vars,
                    context: &context,
                    request: Some(&hitop_core::scripting::SentRequest { url: pre.url.clone(), method: req.method.clone(), headers: sent_headers, body: sent_body }),
                    globals: Some(&scopes.globals),
                },
            )
            .map_err(masked)?;
            super::send::mask_output(&masker, &mut result, &mut post.tests);

            // Persist context updates
            if !pre.context_updates.is_empty() || !post.context_updates.is_empty() {
                let mut contexts = storage::load_contexts()?;
                let merged: serde_json::Map<String, serde_json::Value> = context
                    .iter()
                    .chain(pre.context_updates.iter())
                    .chain(post.context_updates.iter())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if let Some(obj) = contexts.as_object_mut() {
                    obj.insert(context_key, serde_json::Value::Object(merged));
                }
                storage::save_contexts(&contexts)?;
            }

            output::print_status(result.status, result.duration_ms);
            if show_headers {
                output::print_separator();
                output::print_headers(&result.headers);
            }
            output::print_separator();
            if raw {
                println!("{}", result.body);
            } else {
                output::print_body(&result.body);
            }
            if !post.tests.is_empty() {
                output::print_separator();
                output::print_tests(&post.tests);
                if post.tests.iter().any(|t| !t.pass) {
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn describe_auth(auth: &Auth) -> String {
    match auth {
        Auth::None => "none".into(),
        Auth::Bearer { .. } => "Bearer token".into(),
        Auth::Basic { username, .. } => format!("Basic, user {username}"),
        Auth::ApiKey { key, location, .. } => format!("API key {key} in the {}", if *location == hitop_core::model::ApiKeyLocation::Query { "query" } else { "headers" }),
    }
}

fn find_req<'a>(
    col: &'a hitop_core::model::Collection,
    name: &str,
) -> Result<&'a hitop_core::model::Request> {
    col.requests
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| anyhow::anyhow!("request '{}' not found in '{}'", name, col.name))
}

fn timestamp_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
