//! `hitop grpc`: call a gRPC method (unary or server streaming) on a server
//! given as `host:port`, or a saved gRPC request; list and describe the
//! server's services. The schema comes from server reflection, or from
//! `.proto` files (`--proto`, or the saved request's `protos`). Response
//! messages are printed as JSON on stdout; the status and trailers go to
//! stderr (or everything as JSON lines with `--json`). Exit 1 on a status
//! other than OK.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::Args;
use colored::Colorize;
use serde_json::{json, Map, Value};

use hitop_core::{
    grpc::{self, Event},
    model::{Grpc, Header, Request, GRPC},
    secrets::Masker,
};

use super::flags::AuthFlags;
use super::secret::SecretFlags;
use super::source::Source;
use super::ws::session_variables;

#[derive(Args)]
pub struct GrpcArgs {
    /// The server (host:port, http://host:port or https://host:port), or with
    /// -c the name of a saved gRPC request
    target: String,

    /// The method to call (package.Service/Method), or `list [service]`, or
    /// `describe <symbol>`. A saved request calls its own method when left out
    #[arg(value_name = "METHOD | list [SERVICE] | describe SYMBOL")]
    action: Vec<String>,

    /// Collection name or folder path: the saved request, or variables for an address
    #[arg(short = 'c', long)]
    collection: Option<String>,

    /// Environment to use instead of the active one
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// The request message as JSON, @file, or - for stdin (${variables} work);
    /// replaces a saved request's message. Default {}
    #[arg(short = 'd', long = "data", value_name = "JSON")]
    data: Option<String>,

    /// Metadata as "name: value" (repeatable; a name ending in -bin takes base64)
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    headers: Vec<String>,

    /// A .proto file to take the schema from instead of server reflection
    /// (repeatable; replaces a saved request's files)
    #[arg(long = "proto", value_name = "FILE")]
    protos: Vec<PathBuf>,

    /// A directory imports in the .proto files are looked up in (repeatable)
    #[arg(short = 'I', long = "import-path", value_name = "DIR")]
    import_paths: Vec<PathBuf>,

    /// HTTP/2 without TLS for a host:port address
    #[arg(long)]
    plaintext: bool,

    /// Time limit for connecting
    #[arg(long, value_name = "MS", default_value_t = 10_000)]
    connect_timeout: u64,

    /// Deadline for the whole call (sent as grpc-timeout); none by default
    #[arg(long, value_name = "MS")]
    max_time: Option<u64>,

    /// Also print the response headers (to stderr)
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Print every event (headers, message, status) as a line of JSON on stdout
    #[arg(long)]
    json: bool,

    /// Variable as KEY=VALUE (repeatable); overrides the collection and environment
    #[arg(long = "var", value_name = "KEY=VALUE")]
    vars: Vec<String>,

    #[command(flatten)]
    auth: AuthFlags,

    #[command(flatten)]
    secrets: SecretFlags,
}

enum Action {
    Call,
    List(Option<String>),
    Describe(String),
}

/// Metadata as a JSON object; repeated names joined with ", ".
fn metadata_json(pairs: &[(String, String)], masker: &Masker) -> Value {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in pairs {
        let v = masker.mask(v);
        map.entry(k.clone()).and_modify(|old| {
            old.push_str(", ");
            old.push_str(&v);
        }).or_insert(v);
    }
    Value::Object(map.into_iter().map(|(k, v)| (k, Value::String(v))).collect::<Map<_, _>>())
}

/// Every string in a JSON value masked.
fn mask_json(v: Value, masker: &Masker) -> Value {
    match v {
        Value::String(s) => Value::String(masker.mask(&s)),
        Value::Array(a) => Value::Array(a.into_iter().map(|x| mask_json(x, masker)).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (k, mask_json(x, masker))).collect()),
        other => other,
    }
}

fn read_data(arg: &str) -> Result<String> {
    if arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).context("reading the message from stdin")?;
        Ok(s)
    } else if let Some(path) = arg.strip_prefix('@') {
        std::fs::read_to_string(path).with_context(|| format!("reading the message from {path}"))
    } else {
        Ok(arg.to_string())
    }
}

/// The exit code: 0, or 1 when the call ended with a status other than OK
/// or the server couldn't be reached.
pub fn run(args: GrpcArgs) -> Result<i32> {
    let source = args.collection.as_deref().map(Source::open).transpose()?;
    let saved = source.as_ref().and_then(|s| s.collection.requests.iter().find(|r| r.name == args.target)).cloned();
    if let Some(r) = &saved {
        if !r.is_grpc() {
            bail!("request '{}' is not a gRPC request: send it with `hitop requests send`{}", r.name, if r.is_websocket() { " or `hitop ws`" } else { "" });
        }
    } else if let Some(s) = &source {
        if !args.target.contains(':') && !args.target.starts_with("${") {
            bail!("no gRPC request '{}' in '{}', and it is not a host:port address", args.target, s.collection.name);
        }
    }

    let action = match args.action.first().map(String::as_str) {
        None if saved.is_some() => Action::Call,
        None => bail!("give the method to call (package.Service/Method), `list` or `describe <symbol>`"),
        Some("list") if args.action.len() <= 2 => Action::List(args.action.get(1).cloned()),
        Some("describe") if args.action.len() == 2 => Action::Describe(args.action[1].clone()),
        Some("describe") => bail!("describe takes one symbol: a service, method or message name"),
        Some(_) if args.action.len() > 1 => bail!("too many arguments: {}", args.action.join(" ")),
        Some(_) => Action::Call,
    };


    let mut request = saved.unwrap_or_else(|| Request { name: args.target.clone(), url: args.target.clone(), method: "GET".into(), kind: Some(GRPC.into()), ..Default::default() });
    for h in &args.headers {
        let Some((k, v)) = h.split_once(':') else { bail!("--header takes \"name: value\", got '{h}'") };
        request.headers.push(Header::new(k.trim(), v.trim()));
    }
    if let Some(auth) = args.auth.auth()? {
        request.auth = Some(auth);
    }
    let g = request.grpc.get_or_insert_with(Grpc::default);
    if let (Action::Call, Some(m)) = (&action, args.action.first()) {
        let (service, method) = grpc::split_method(m).ok_or_else(|| anyhow!("'{m}' is not a method: give package.Service/Method"))?;
        g.service = service;
        g.method = method;
    }
    if let Some(d) = &args.data {
        g.message = read_data(d)?;
    }
    if args.plaintext {
        g.plaintext = true;
    }
    let mut refs = std::collections::BTreeSet::new();
    hitop_core::secret_refs::find_in_request(&request, &mut refs);
    let (vars, masker) = session_variables(source.as_ref(), args.env.as_deref(), &args.vars, &args.secrets, refs)?;
    let masked = |e: anyhow::Error| anyhow!(masker.mask(&format!("{e:#}")));
    let default_auth = source.as_ref().and_then(|s| s.collection.auth.clone());
    let base = source.as_ref().and_then(|s| s.folder()).map(PathBuf::from);
    let mut target = grpc::target(&request, default_auth.as_ref(), &vars, base.as_deref()).map_err(masked)?;
    // Paths given on the command line are relative to where hitop runs.
    if !args.protos.is_empty() {
        target.protos = args.protos.clone();
    }
    target.import_paths.extend(args.import_paths.iter().cloned());
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let out = Out { json: args.json, verbose: args.verbose, masker: &masker };
    rt.block_on(async {
        let connect = Some(Duration::from_millis(args.connect_timeout));
        let channel = match grpc::connect(&target, connect).await {
            Ok(c) => c,
            Err(e) if matches!(action, Action::Call) => {
                out.failed(&format!("{e:#}"));
                return Ok(1);
            }
            Err(e) => return Err(masked(e)),
        };
        let max_time = args.max_time.map(Duration::from_millis);
        let (pool, _) = grpc::schema(&target, &channel, max_time.or(connect)).await.map_err(masked)?;
        match action {
            Action::List(service) => list(&out, &pool, service.as_deref()).map(|()| 0),
            Action::Describe(symbol) => describe(&out, &pool, &symbol).map(|()| 0),
            Action::Call => {
                let mut events = grpc::call(channel, &pool, &target, max_time).await.map_err(masked)?;
                let mut code = 1;
                while let Some(e) = events.recv().await {
                    if let Event::Status(s) = &e {
                        code = i32::from(!s.is_ok());
                    }
                    out.event(&e);
                }
                Ok(code)
            }
        }
    })
}

struct Out<'a> {
    json: bool,
    verbose: bool,
    masker: &'a Masker,
}

impl Out<'_> {
    fn m(&self, s: &str) -> String {
        self.masker.mask(s)
    }

    fn line(&self, v: Value) {
        let mut v = mask_json(v, self.masker);
        if let Value::Object(map) = &mut v {
            map.insert("time".into(), Value::String(super::ws::now()));
        }
        println!("{v}");
    }

    fn event(&self, e: &Event) {
        match (e, self.json) {
            (Event::Headers(h), true) => self.line(json!({ "event": "headers", "metadata": metadata_json(h, self.masker) })),
            (Event::Headers(h), false) => {
                if self.verbose {
                    for (k, v) in h {
                        eprintln!("{} {}", format!("{k}:").dimmed(), self.m(v));
                    }
                }
            }
            (Event::Message(m), true) => self.line(json!({ "event": "message", "data": m })),
            (Event::Message(m), false) => println!("{}", self.m(&serde_json::to_string_pretty(m).unwrap_or_default())),
            (Event::Status(s), true) => self.line(json!({ "event": "status", "code": s.code, "name": s.name, "message": s.message, "trailers": metadata_json(&s.trailers, self.masker) })),
            (Event::Status(s), false) => {
                if s.is_ok() {
                    eprintln!("{}", "Status: OK".green());
                } else {
                    let msg = if s.message.is_empty() { String::new() } else { format!(": {}", self.m(&s.message)) };
                    eprintln!("{}", format!("Status: {} ({}){msg}", s.name, s.code).red());
                }
                for (k, v) in &s.trailers {
                    eprintln!("{} {}", format!("{k}:").dimmed(), self.m(v));
                }
            }
        }
    }

    /// The server couldn't be reached: UNAVAILABLE, as a call's status.
    fn failed(&self, message: &str) {
        self.event(&Event::Status(grpc::Status { code: 14, name: grpc::code_name(14).into(), message: message.to_string(), trailers: vec![] }));
    }
}

fn list(out: &Out, pool: &grpc::DescriptorPool, service: Option<&str>) -> Result<()> {
    match service {
        None => {
            let services = grpc::services(pool);
            if out.json {
                println!("{}", serde_json::to_string_pretty(&services)?);
            } else {
                for s in services {
                    println!("{}", s.name);
                }
            }
        }
        Some(name) => {
            let s = grpc::find_service(pool, name)?;
            let info = grpc::services(pool).into_iter().find(|i| i.name == s.full_name()).ok_or_else(|| anyhow!("no service {name}"))?;
            if out.json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                for m in info.methods {
                    println!("{}", m.full_name);
                }
            }
        }
    }
    Ok(())
}

fn describe(out: &Out, pool: &grpc::DescriptorPool, symbol: &str) -> Result<()> {
    let text = grpc::describe(pool, symbol)?;
    // A message's template, or a method's request template.
    let template = pool
        .get_message_by_name(symbol.trim_start_matches('.'))
        .or_else(|| grpc::split_method(symbol).and_then(|(s, m)| grpc::find_method(pool, &s, &m).ok()).map(|m| m.input()))
        .map(|m| grpc::template(&m));
    if out.json {
        let mut v = json!({ "symbol": symbol, "description": text });
        if let Some(t) = template {
            v["template"] = t;
        }
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        println!("{text}");
        if let Some(t) = template {
            println!("\n{}\n{}", "Message template:".dimmed(), serde_json::to_string_pretty(&t)?);
        }
    }
    Ok(())
}
