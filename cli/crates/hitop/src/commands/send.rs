use anyhow::Result;
use clap::Args;
use std::collections::HashMap;

use hitop_core::{
    http,
    model::{Header, HistoryItem},
    params::append_query,
    prepare::{prepare, variables, Draft},
    scripting::run_post_request_script,
    storage,
};

use super::flags::{query_pair, script_arg, AuthFlags};
use super::source::Source;
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

    /// Query parameter appended to the URL, as key=value (repeatable)
    #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
    query: Vec<String>,

    #[command(flatten)]
    auth: AuthFlags,

    /// Collection name or folder path to load variables and context from
    #[arg(short = 'c', long)]
    collection: Option<String>,

    /// Environment to use instead of the active one
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// Pre-request script, inline or @file
    #[arg(long, value_name = "JS|@FILE")]
    pre_script: Option<String>,

    /// Post-request script (assert.* checks included), inline or @file
    #[arg(long, value_name = "JS|@FILE")]
    post_script: Option<String>,

    /// Show response headers
    #[arg(long)]
    show_headers: bool,

    /// Disable syntax highlighting
    #[arg(long)]
    raw: bool,
}

pub fn run(args: SendArgs) -> Result<()> {
    let mut collection_vars: HashMap<String, String> = HashMap::new();
    let mut context: HashMap<String, serde_json::Value> = HashMap::new();

    let source = args.collection.as_deref().map(Source::open).transpose()?;
    if let Some(source) = &source {
        collection_vars = source.collection.variables.clone();
        // Load persisted context for this collection
        let contexts = storage::load_contexts()?;
        if let Some(ctx_map) = contexts.get(source.context_key()).and_then(|v| v.as_object()) {
            context = ctx_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        }
    }
    let environment = match &source {
        Some(s) => s.environment(args.env.as_deref())?,
        None => storage::load_environments()?.pick(args.env.as_deref())?.cloned(),
    };
    let variables = variables(&collection_vars, environment.as_ref());

    let headers: Vec<Header> = args
        .headers
        .iter()
        .filter_map(|h| {
            let (k, v) = h.split_once(':')?;
            Some(Header::new(k.trim(), v.trim()))
        })
        .collect();
    let url = append_query(&args.url, &args.query.iter().map(|q| query_pair(q)).collect::<Vec<_>>());
    let body = args.body.clone().unwrap_or_default();
    let pre_script = script_arg(&args.pre_script)?;
    let post_script = script_arg(&args.post_script)?;
    let auth = args.auth.auth()?;

    let draft = Draft { method: &args.method, url: &url, headers: &headers, body: &body, pre_request_script: &pre_script, auth: auth.as_ref() };
    let pre = prepare(&draft, &variables, &context)?;
    context.extend(pre.context_updates.clone());

    let result = http::execute(&args.method, &pre.url, &pre.headers, &pre.body)?;

    let post = run_post_request_script(
        &post_script,
        &result.body,
        &result.headers,
        result.status,
        result.duration_ms,
        &variables,
        &context,
    )?;

    // Persist updated context if tied to a collection
    if let Some(source) = &source {
        let mut contexts = storage::load_contexts()?;
        let merged: serde_json::Map<String, serde_json::Value> = context
            .iter()
            .chain(post.context_updates.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(obj) = contexts.as_object_mut() {
            obj.insert(source.context_key(), serde_json::Value::Object(merged));
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

    if !post.tests.is_empty() {
        output::print_separator();
        output::print_tests(&post.tests);
        if post.tests.iter().any(|t| !t.pass) {
            std::process::exit(1);
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
