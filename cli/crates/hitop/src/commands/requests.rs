use anyhow::{bail, Result};
use clap::Subcommand;
use colored::Colorize;

use hitop_core::{model::{Header, Request}, storage};

#[derive(Subcommand)]
pub enum RequestsCmd {
    /// List requests in a collection
    List {
        collection: String,
    },
    /// Add a request to a collection
    Add {
        collection: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "GET")]
        method: String,
    },
    /// Show a request's details
    Show {
        collection: String,
        request: String,
    },
    /// Delete a request
    Delete {
        collection: String,
        request: String,
    },
    /// Rename a request
    Rename {
        collection: String,
        request: String,
        new_name: String,
    },
    /// Load and immediately send a request from a collection
    Send {
        collection: String,
        request: String,
        /// Show response headers
        #[arg(long)]
        show_headers: bool,
        /// Disable syntax highlighting
        #[arg(long)]
        raw: bool,
    },
}

pub fn run(cmd: RequestsCmd) -> Result<()> {
    let mut collections = storage::load_collections()?;

    match cmd {
        RequestsCmd::List { collection } => {
            let col = find_col(&collections, &collection)?;
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

        RequestsCmd::Add { collection, name, url, method } => {
            let col = find_col_mut(&mut collections, &collection)?;
            let req = Request {
                id: timestamp_id(),
                name: name.clone(),
                url,
                method,
                headers: vec![Header { name: String::new(), value: String::new() }],
                body: String::new(),
                pre_request_script: String::new(),
                post_request_script: String::new(),
            };
            col.requests.push(req);
            storage::save_collections(&collections)?;
            println!("{} Added request '{}' to '{}'", "✓".green(), name.bold(), collection.bold());
        }

        RequestsCmd::Show { collection, request } => {
            let col = find_col(&collections, &collection)?;
            let req = find_req(col, &request)?;
            println!("{}: {}", "Name".bold(), req.name);
            println!("{}: {}", "Method".bold(), req.method);
            println!("{}: {}", "URL".bold(), req.url);
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
            let col = find_col_mut(&mut collections, &collection)?;
            let len_before = col.requests.len();
            col.requests.retain(|r| r.name != request);
            if col.requests.len() == len_before {
                bail!("request '{}' not found in '{}'", request, collection);
            }
            storage::save_collections(&collections)?;
            println!("{} Deleted request '{}'", "✓".green(), request.bold());
        }

        RequestsCmd::Rename { collection, request, new_name } => {
            let col = find_col_mut(&mut collections, &collection)?;
            let req = col
                .requests
                .iter_mut()
                .find(|r| r.name == request)
                .ok_or_else(|| anyhow::anyhow!("request '{}' not found", request))?;
            req.name = new_name.clone();
            storage::save_collections(&collections)?;
            println!("{} Renamed '{}' → '{}'", "✓".green(), request.bold(), new_name.bold());
        }

        RequestsCmd::Send { collection, request, show_headers, raw } => {
            use hitop_core::{http, interpolation::interpolate, scripting::{run_pre_request_script, run_post_request_script, PreRequestCtx}};
            use crate::output;

            let col = find_col(&collections, &collection)?.clone();
            let req = find_req(&col, &request)?.clone();
            let vars = &col.variables;

            let url = interpolate(&req.url, vars)?;
            let headers: Vec<(String, String)> = req.headers
                .iter()
                .filter(|h| !h.name.is_empty())
                .map(|h| {
                    Ok((
                        interpolate(&h.name, vars)?,
                        interpolate(&h.value, vars)?,
                    ))
                })
                .collect::<Result<_>>()?;
            let body = interpolate(&req.body, vars).unwrap_or_else(|_| req.body.clone());

            // Load context
            let contexts = storage::load_contexts()?;
            let context: std::collections::HashMap<String, serde_json::Value> = contexts
                .get(&collection)
                .and_then(|v| v.as_object())
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            let pre_ctx = PreRequestCtx {
                url,
                method: req.method.clone(),
                headers,
                body,
                variables: vars.clone(),
                context: context.clone(),
            };

            let pre = run_pre_request_script(&req.pre_request_script, pre_ctx)?;
            let result = http::execute(&req.method, &pre.url, &pre.headers, &pre.body)?;

            let post = run_post_request_script(
                &req.post_request_script,
                &result.body,
                &result.headers,
                result.status,
                vars,
                &context,
            )?;

            // Persist context updates
            if !post.context_updates.is_empty() {
                let mut contexts = storage::load_contexts()?;
                let merged: serde_json::Map<String, serde_json::Value> = context
                    .iter()
                    .chain(pre.context_updates.iter())
                    .chain(post.context_updates.iter())
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if let Some(obj) = contexts.as_object_mut() {
                    obj.insert(collection.clone(), serde_json::Value::Object(merged));
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
        }
    }

    Ok(())
}

fn find_col<'a>(
    collections: &'a [hitop_core::model::Collection],
    name: &str,
) -> Result<&'a hitop_core::model::Collection> {
    collections
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", name))
}

fn find_col_mut<'a>(
    collections: &'a mut Vec<hitop_core::model::Collection>,
    name: &str,
) -> Result<&'a mut hitop_core::model::Collection> {
    collections
        .iter_mut()
        .find(|c| c.name == name)
        .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", name))
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
