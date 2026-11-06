use anyhow::Result;
use clap::Args;
use std::collections::HashMap;

use hitop_core::{
    form,
    graphql::Operation,
    http,
    interpolation::interpolate,
    model::{Header, HistoryItem},
    params::append_query,
    prepare::{prepare_in, variables, Draft, Scopes},
    scripting::{run_post_request_script_with, PostRequestCtx},
    storage,
};

use super::cookies::{self, CookieFlags};
use super::flags::{query_pair, script_arg, AuthFlags};
use super::secret::{self, SecretFlags};
use super::source::Source;
use crate::output;

#[derive(Args)]
pub struct SendArgs {
    /// Target URL
    url: String,

    /// HTTP method (default GET, or POST with -F)
    #[arg(short = 'X', long)]
    method: Option<String>,

    /// Headers in "Name: Value" format (repeatable)
    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    /// Request body
    #[arg(short = 'b', long)]
    body: Option<String>,

    /// Send a GraphQL query read from FILE (- reads standard input), as a
    /// JSON POST of {query, variables, operationName}
    #[arg(long, value_name = "FILE|-", conflicts_with_all = ["body", "form"])]
    graphql: Option<String>,

    /// GraphQL variables: a JSON object, inline or @file; ${variables} work in it
    #[arg(long, value_name = "JSON|@FILE", requires = "graphql")]
    graphql_vars: Option<String>,

    /// GraphQL operation to run, when the query has more than one
    #[arg(long, value_name = "NAME", requires = "graphql")]
    operation_name: Option<String>,

    /// Form field, sent as multipart/form-data: NAME=VALUE, or NAME=@FILE
    /// with an optional ;type=MIME (repeatable)
    #[arg(short = 'F', long = "form", value_name = "NAME=VALUE|NAME=@FILE", conflicts_with = "body")]
    form: Vec<String>,

    /// Send the -F fields as application/x-www-form-urlencoded instead (no files)
    #[arg(long, requires = "form")]
    urlencoded: bool,

    /// Query parameter appended to the URL, as key=value (repeatable)
    #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
    query: Vec<String>,

    #[command(flatten)]
    cookies: CookieFlags,

    #[command(flatten)]
    auth: AuthFlags,

    #[command(flatten)]
    secrets: SecretFlags,

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
    let workspace = source.as_ref().map(|s| s.workspace()).unwrap_or_else(|| hitop_core::secrets::GLOBAL_WORKSPACE.into());
    let mut secrets = secret::resolve(&workspace, environment.as_ref(), &args.secrets)?;
    let mut variables = variables(&collection_vars, environment.as_ref());
    secrets.apply(&mut variables);
    // Globals sit below everything else: a folder's globals.yaml, else global storage's.
    let globals = match &source {
        Some(s) => s.globals()?,
        None => storage::load_globals()?,
    };
    let (globals, global_secrets) = secret::resolve_globals(&workspace, &globals, &args.secrets)?;
    let mut scopes = Scopes { globals, upper: variables };

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

    let query = match args.graphql.as_deref() {
        None => None,
        Some("-") => Some(std::io::read_to_string(std::io::stdin()).map_err(|e| anyhow::anyhow!("--graphql -: reading standard input: {e}"))?),
        Some(path) => Some(std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("--graphql: reading {path}: {e}"))?),
    };
    let graphql_vars = script_arg(&args.graphql_vars)?;
    let operation_name = args.operation_name.clone().unwrap_or_default();
    let graphql = query.as_deref().map(|query| Operation { query, variables: &graphql_vars, operation_name: &operation_name });

    let method = args.method.clone().unwrap_or_else(|| if args.form.is_empty() && graphql.is_none() { "GET" } else { "POST" }.into());
    let draft = Draft { method: &method, url: &url, headers: &headers, body: &body, pre_request_script: &pre_script, auth: auth.as_ref(), graphql };
    // ${secret:vault|aws|azure|op:...} in the request or in a variable's value.
    let auth_json = auth.as_ref().map(serde_json::to_value).transpose()?.unwrap_or_default();
    let header_texts = headers.iter().flat_map(|h| [h.name.as_str(), h.value.as_str()]);
    let graphql_texts = [query.as_deref().unwrap_or_default(), graphql_vars.as_str(), operation_name.as_str()];
    let mut refs = secret::refs_in([url.as_str(), body.as_str()].into_iter().chain(header_texts).chain(graphql_texts).chain(args.form.iter().map(String::as_str)));
    hitop_core::secret_refs::find_in_json(&auth_json, &mut refs);
    secret::refs_in_vars([&scopes.upper, &scopes.globals], &mut refs);
    secret::add_refs(&mut secrets, &refs, &args.secrets)?;
    secret::fill_refs(&secrets, &mut scopes);
    let masker = secrets.masker_with(&global_secrets);
    let oauth = hitop_core::oauth2::fetch_once(draft.auth, &scopes.merged(), None).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    let mut draft = draft;
    if let Some(a) = &oauth {
        draft.auth = Some(a);
    }
    let pre = prepare_in(&draft, &scopes, &context).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    context.extend(pre.context_updates.clone());
    scopes.apply(&pre.variables, &pre.globals);
    let variables = scopes.merged();

    let mut headers = pre.headers.clone();
    let mut body = pre.body.clone().into_bytes();
    if !args.form.is_empty() {
        let built = form_body(&args.form, args.urlencoded, &variables)?;
        if !headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("content-type") && !v.is_empty()) {
            headers.push(("Content-Type".into(), built.content_type));
        }
        body = built.body;
    } else {
        // A raw body says what it is, as in every other hitop.
        hitop_core::sending::add_content_type(&method, &mut headers, &pre.body);
    }
    hitop_core::aws_sigv4::sign_if_aws(auth.as_ref(), &|s| hitop_core::interpolation::interpolate(s, &variables), &method, &pre.url, &mut headers, &body).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    let (headers_sent, body_sent) = (headers.clone(), String::from_utf8(body.clone()).unwrap_or_default());
    let mut jar = cookies::open(&workspace, args.cookies)?;
    let opts = http::Options { timeout: None, jar: jar.as_mut() };
    let mut result = http::execute_opts(&method, &pre.url, &headers, body.clone(), opts).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    // Digest: the server's 401 challenge answered once.
    let fill = |s: &str| hitop_core::interpolation::interpolate(s, &variables);
    if let Some(h2) = hitop_core::digest_auth::retry_headers(auth.as_ref(), &fill, result.status, &result.headers, &method, &pre.url, &headers).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))? {
        result = http::execute_opts(&method, &pre.url, &h2, body, http::Options { timeout: None, jar: jar.as_mut() }).map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    }
    cookies::save(&workspace, jar.as_ref())?;

    let sent = hitop_core::scripting::SentRequest { url: pre.url.clone(), method: method.clone(), headers: headers_sent, body: body_sent };
    let mut post = run_post_request_script_with(
        &post_script,
        &PostRequestCtx {
            response_body: &result.body,
            response_headers: &result.headers,
            status_code: result.status,
            duration_ms: result.duration_ms,
            variables: &variables,
            context: &context,
            request: Some(&sent),
            globals: Some(&scopes.globals),
        },
    )
    .map_err(|e| anyhow::anyhow!(masker.mask(&format!("{e:#}"))))?;
    // Scripts saw the real values; whatever is printed or kept doesn't.
    mask_output(&masker, &mut result, &mut post.tests);

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
        url: masker.mask(&pre.url),
        method: method.clone(),
        status_code: result.status,
        success: result.status < 400,
        collection_name: args.collection.clone(),
        request_name: None,
        duration_ms: result.duration_ms,
        extra: Default::default(),
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

/// The body for `-F` fields: multipart, or urlencoded with `--urlencoded`.
/// Values get `${variables}` filled in; files are read as they are.
fn form_body(fields: &[String], urlencoded: bool, variables: &HashMap<String, String>) -> Result<form::Built> {
    let mut parts = vec![];
    for f in fields {
        let Some((name, value)) = f.split_once('=').filter(|(n, _)| !n.is_empty()) else {
            anyhow::bail!("-F takes NAME=VALUE or NAME=@FILE, got '{f}'");
        };
        let name = interpolate(name, variables)?;
        match value.strip_prefix('@') {
            Some(spec) => {
                if urlencoded {
                    anyhow::bail!("--urlencoded can't send files (-F {name}=@...)");
                }
                let (path, content_type) = match spec.rsplit_once(";type=") {
                    Some((p, t)) => (p, t.to_string()),
                    None => (spec, guess_type(spec).to_string()),
                };
                let data = std::fs::read(path).map_err(|e| anyhow::anyhow!("-F {name}: reading {path}: {e}"))?;
                let filename = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                parts.push(form::Part::File { name, filename, content_type, data });
            }
            None => parts.push(form::Part::Text { name, value: interpolate(value, variables)? }),
        }
    }
    if urlencoded {
        let pairs: Vec<(String, String)> = parts.into_iter().filter_map(|p| match p {
            form::Part::Text { name, value } => Some((name, value)),
            form::Part::File { .. } => None,
        }).collect();
        return Ok(form::urlencoded(&pairs));
    }
    Ok(form::multipart(&parts))
}

/// A content type from a file's extension (application/octet-stream when unknown).
fn guess_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "json" => "application/json",
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Masks secret values in a response and in assertion results before they
/// are printed.
pub fn mask_output(masker: &hitop_core::secrets::Masker, result: &mut hitop_core::model::RequestResult, tests: &mut [hitop_core::model::TestResult]) {
    if masker.is_empty() {
        return;
    }
    result.body = masker.mask(&result.body);
    for v in result.headers.values_mut() {
        *v = masker.mask(v);
    }
    for t in tests {
        t.name = masker.mask(&t.name);
        t.detail = masker.mask(&t.detail);
    }
}

fn timestamp_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
