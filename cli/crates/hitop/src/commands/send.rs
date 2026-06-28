use anyhow::{bail, Result};
use clap::Args;
use std::collections::HashMap;

use hitop_core::{
    http,
    interpolation::interpolate,
    model::HistoryItem,
    scripting::{run_post_request_script, run_pre_request_script, PreRequestCtx},
    storage,
};

use crate::output;

#[derive(Args)]
pub struct SendArgs {
    /// Target URL
    url: String,

    /// HTTP method
    #[arg(short = 'X', long, default_value = "GET")]
    method: String,

    /// Headers in "Name: Value" format (repeatable)
    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    /// Request body
    #[arg(short = 'b', long)]
    body: Option<String>,

    /// Collection name to load variables and context from
    #[arg(short = 'c', long)]
    collection: Option<String>,

    /// Show response headers
    #[arg(long)]
    show_headers: bool,

    /// Disable syntax highlighting
    #[arg(long)]
    raw: bool,
}

pub fn run(args: SendArgs) -> Result<()> {
    let mut variables: HashMap<String, String> = HashMap::new();
    let mut context: HashMap<String, serde_json::Value> = HashMap::new();
    let pre_script = String::new();
    let post_script = String::new();

    if let Some(col_name) = &args.collection {
        let collections = storage::load_collections()?;
        match collections.iter().find(|c| c.name == *col_name) {
            Some(col) => {
                variables = col.variables.clone();
                // Load persisted context for this collection
                let contexts = storage::load_contexts()?;
                if let Some(ctx_map) = contexts.get(col_name).and_then(|v| v.as_object()) {
                    context = ctx_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                }
            }
            None => bail!("collection '{}' not found", col_name),
        }
    }

    let url = interpolate(&args.url, &variables)?;

    let headers: Vec<(String, String)> = args
        .headers
        .iter()
        .filter_map(|h| {
            let mut parts = h.splitn(2, ':');
            let k = parts.next()?.trim().to_string();
            let v = parts.next()?.trim().to_string();
            Some((k, v))
        })
        .collect();

    let body = args.body.clone().unwrap_or_default();

    // Pre-request script (not available in one-off send, only collection context)
    let pre_ctx = PreRequestCtx {
        url,
        method: args.method.clone(),
        headers,
        body,
        variables: variables.clone(),
        context: context.clone(),
    };

    let pre = run_pre_request_script(&pre_script, pre_ctx)?;
    for (k, v) in &pre.context_updates {
        context.insert(k.clone(), v.clone());
    }

    let result = http::execute(&args.method, &pre.url, &pre.headers, &pre.body)?;

    // Post-request script
    let post = run_post_request_script(
        &post_script,
        &result.body,
        &result.headers,
        result.status,
        &variables,
        &context,
    )?;

    // Persist updated context if tied to a collection
    if let Some(col_name) = &args.collection {
        let mut contexts = storage::load_contexts()?;
        let merged: serde_json::Map<String, serde_json::Value> = context
            .iter()
            .chain(post.context_updates.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(obj) = contexts.as_object_mut() {
            obj.insert(col_name.clone(), serde_json::Value::Object(merged));
        }
        storage::save_contexts(&contexts)?;
    }

    // Add to history
    let mut history = storage::load_history()?;
    history.push(HistoryItem {
        id: timestamp_id(),
        timestamp: timestamp_id(),
        url: pre.url.clone(),
        method: args.method.clone(),
        status_code: result.status,
        success: result.status < 400,
        collection_name: args.collection.clone(),
        request_name: None,
        duration_ms: result.duration_ms,
    });
    storage::save_history(&history)?;

    // Output
    output::print_status(result.status, result.duration_ms);

    if args.show_headers {
        output::print_separator();
        output::print_headers(&result.headers);
    }

    output::print_separator();

    if args.raw {
        println!("{}", result.body);
    } else {
        output::print_body(&result.body);
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
