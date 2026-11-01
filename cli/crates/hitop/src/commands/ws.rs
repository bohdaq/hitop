//! `hitop ws`: open a WebSocket connection (a URL or a saved WebSocket
//! request), send messages typed on stdin or given as flags, and print what
//! the server sends. For scripts: `--send`/`--message`, `--count`, `--wait`
//! and `--json` (one JSON object per event).
use std::collections::HashMap;
use std::io::{BufRead, IsTerminal};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use base64::Engine;
use clap::Args;
use colored::Colorize;
use serde_json::{json, Value};

use hitop_core::{
    interpolation::interpolate,
    model::{Header, Request, WebSocket, WEBSOCKET},
    prepare::{variables, Scopes},
    secrets::Masker,
    storage,
    websocket::{self, Data, Event},
};

use super::flags::AuthFlags;
use super::secret::{self, SecretFlags};
use super::source::Source;

#[derive(Args)]
pub struct WsArgs {
    /// A ws:// or wss:// URL, or with -c the name of a saved WebSocket request
    target: String,

    /// Collection name or folder path: the saved request, or variables for a URL
    #[arg(short = 'c', long)]
    collection: Option<String>,

    /// Environment to use instead of the active one
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// Handshake header in "Name: Value" format (repeatable)
    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    /// Subprotocol to offer, in order (repeatable; replaces the saved ones)
    #[arg(long = "protocol", value_name = "NAME")]
    protocols: Vec<String>,

    /// Send the saved request's message NAME after connecting (repeatable)
    #[arg(short = 'm', long = "message", value_name = "NAME")]
    messages: Vec<String>,

    /// Send TEXT after connecting, after any --message (repeatable; ${variables} work).
    /// Without --send or --message, lines typed on stdin are sent
    #[arg(short = 's', long = "send", value_name = "TEXT")]
    send: Vec<String>,

    /// Exit once the server has sent N messages (exit 1 if the connection
    /// closes or --wait runs out first)
    #[arg(short = 'n', long = "count", value_name = "N")]
    count: Option<usize>,

    /// Keep listening this long after the last message is sent (or stdin
    /// ends), then close [default: 1000, or 30000 with --count]
    #[arg(long, value_name = "MS")]
    wait: Option<u64>,

    /// Time limit for the handshake
    #[arg(long, value_name = "MS", default_value_t = 30_000)]
    timeout: u64,

    /// Print every event (open, sent, received, closed, error) as a line of JSON on stdout
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

/// How events are printed: received messages on stdout, the rest on stderr;
/// or everything as JSON lines on stdout. Every string is masked.
struct Out {
    json: bool,
    tty: bool,
    masker: Masker,
}

pub(crate) fn now() -> String {
    let t = SystemTime::now();
    let ms = t.duration_since(UNIX_EPOCH).map(|d| d.subsec_millis()).unwrap_or(0);
    let s = hitop_core::report::rfc3339(t);
    format!("{}.{ms:03}Z", s.trim_end_matches('Z'))
}

impl Out {
    fn m(&self, s: &str) -> String {
        self.masker.mask(s)
    }

    fn line(&self, mut v: Value) {
        if let Value::Object(map) = &mut v {
            for val in map.values_mut() {
                if let Value::String(s) = val {
                    *s = self.m(s);
                }
            }
            map.insert("time".into(), Value::String(now()));
        }
        println!("{v}");
    }

    fn opened(&self, url: &str, o: &websocket::Opened) {
        if self.json {
            self.line(json!({ "event": "open", "url": url, "status": o.status, "protocol": o.protocol }));
        } else {
            let proto = o.protocol.as_deref().map(|p| format!(", subprotocol {p}")).unwrap_or_default();
            eprintln!("{}", format!("* connected to {}{proto}", self.m(url)).dimmed());
        }
    }

    fn sent(&self, text: &str) {
        if self.json {
            self.line(json!({ "event": "sent", "data": text }));
        } else {
            eprintln!("{} {}", ">".cyan(), self.m(text));
        }
    }

    fn received(&self, data: &Data) {
        match (data, self.json) {
            (Data::Text(t), true) => self.line(json!({ "event": "received", "data": t })),
            (Data::Binary(b), true) => self.line(json!({ "event": "received", "binary": base64::engine::general_purpose::STANDARD.encode(b) })),
            (Data::Text(t), false) if self.tty => println!("{} {}", "<".green(), self.m(t)),
            (Data::Text(t), false) => println!("{}", self.m(t)),
            (Data::Binary(b), false) => {
                let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
                println!("{}[binary, {} bytes] {hex}", if self.tty { "< " } else { "" }, b.len());
            }
        }
    }

    fn closed(&self, code: Option<u16>, reason: &str) {
        if self.json {
            self.line(json!({ "event": "closed", "code": code, "reason": reason }));
        } else {
            let code = code.map(|c| format!(" {c}")).unwrap_or_default();
            let reason = if reason.is_empty() { String::new() } else { format!(" {}", self.m(reason)) };
            eprintln!("{}", format!("* closed{code}{reason}").dimmed());
        }
    }

    fn error(&self, message: &str) {
        if self.json {
            self.line(json!({ "event": "error", "message": message }));
        } else {
            eprintln!("{} {}", "* error:".red(), self.m(message));
        }
    }
}

fn var_pairs(vars: &[String]) -> Result<HashMap<String, String>> {
    vars.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
            _ => bail!("--var takes KEY=VALUE, got '{s}'"),
        })
        .collect()
}

/// Variables for a connection or call (`hitop ws`, `hitop grpc`), as for
/// `hitop send`: globals < collection < environment < --var < secrets; and
/// the masker for every secret value resolved.
pub(crate) fn session_variables(source: Option<&Source>, env: Option<&str>, vars: &[String], flags: &SecretFlags) -> Result<(HashMap<String, String>, Masker)> {
    let environment = match source {
        Some(s) => s.environment(env)?,
        None => storage::load_environments()?.pick(env)?.cloned(),
    };
    let workspace = source.map(|s| s.workspace()).unwrap_or_else(|| hitop_core::secrets::GLOBAL_WORKSPACE.into());
    let secrets = secret::resolve(&workspace, environment.as_ref(), flags)?;
    let collection_vars = source.map(|s| s.collection.variables.clone()).unwrap_or_default();
    let mut upper = variables(&collection_vars, environment.as_ref());
    upper.extend(var_pairs(vars)?);
    secrets.apply(&mut upper);
    let globals = match source {
        Some(s) => s.globals()?,
        None => storage::load_globals()?,
    };
    let (globals, global_secrets) = secret::resolve_globals(&workspace, &globals, flags)?;
    Ok((Scopes { globals, upper }.merged(), secrets.masker_with(&global_secrets)))
}

/// The exit code: 0, or 1 when the connection failed or --count wasn't reached.
pub fn run(args: WsArgs) -> Result<i32> {
    let source = args.collection.as_deref().map(Source::open).transpose()?;
    let saved = source.as_ref().and_then(|s| s.collection.requests.iter().find(|r| r.name == args.target)).cloned();
    if let Some(r) = &saved {
        if !r.is_websocket() {
            bail!("request '{}' is an HTTP request: send it with `hitop requests send`", r.name);
        }
    } else if !args.target.contains("://") && !args.target.starts_with("${") {
        match &source {
            Some(s) => bail!("no WebSocket request '{}' in '{}', and it is not a ws:// or wss:// URL", args.target, s.collection.name),
            None => bail!("'{}' is not a ws:// or wss:// URL (give -c <collection> to open a saved request)", args.target),
        }
    }
    if saved.is_none() && !args.messages.is_empty() {
        bail!("--message sends a saved request's message: give -c <collection> and the request's name");
    }

    let (vars, masker) = session_variables(source.as_ref(), args.env.as_deref(), &args.vars, &args.secrets)?;
    let out = Out { json: args.json, tty: std::io::stdout().is_terminal(), masker };
    let masked = |e: anyhow::Error| anyhow!(out.m(&format!("{e:#}")));

    let mut request = saved.unwrap_or_else(|| Request { name: args.target.clone(), url: args.target.clone(), method: "GET".into(), kind: Some(WEBSOCKET.into()), ..Default::default() });
    for h in &args.headers {
        let Some((k, v)) = h.split_once(':') else { bail!("--header takes \"Name: Value\", got '{h}'") };
        request.headers.push(Header::new(k.trim(), v.trim()));
    }
    if let Some(auth) = args.auth.auth()? {
        request.auth = Some(auth);
    }
    if !args.protocols.is_empty() {
        request.websocket.get_or_insert_with(WebSocket::default).subprotocols = args.protocols.clone();
    }
    let default_auth = source.as_ref().and_then(|s| s.collection.auth.clone());
    let target = websocket::target(&request, default_auth.as_ref(), &vars).map_err(masked)?;
    let mut queue = args.messages.iter().map(|name| websocket::message(&request, name, &vars)).collect::<Result<Vec<_>>>().map_err(masked)?;
    queue.extend(args.send.iter().map(|t| interpolate(t, &vars)).collect::<Result<Vec<_>>>().map_err(masked)?);

    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(session(&out, &target, queue, &args))
}

async fn session(out: &Out, target: &websocket::Target, queue: Vec<String>, args: &WsArgs) -> Result<i32> {
    let conn = match websocket::connect(target, Some(Duration::from_millis(args.timeout))).await {
        Ok(c) => c,
        Err(e) => {
            out.error(&format!("{e:#}"));
            return Ok(1);
        }
    };
    out.opened(&target.url, &conn.opened);
    let websocket::Connection { sender, mut events, .. } = conn;
    let wait = Duration::from_millis(args.wait.unwrap_or(if args.count.is_some() { 30_000 } else { 1_000 }));
    let enough = |n: usize| args.count.is_some_and(|c| n >= c);
    let short = |n: usize| args.count.is_some_and(|c| n < c);

    // Typed lines (or piped ones) when no message was given as a flag.
    let mut lines = if queue.is_empty() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Some(rx)
    } else {
        None
    };
    for text in queue {
        sender.send(Data::Text(text.clone()))?;
        out.sent(&text);
    }
    let mut deadline = lines.is_none().then(|| Instant::now() + wait);
    let mut received = 0usize;
    if enough(received) {
        sender.close();
    }
    loop {
        let sleep = deadline.map(|d| tokio::time::sleep_until(d.into()));
        tokio::select! {
            line = async { lines.as_mut().unwrap().recv().await }, if lines.is_some() => match line {
                Some(line) if line.trim().is_empty() => {}
                Some(line) => {
                    let text = line.trim_end_matches('\r').to_string();
                    if sender.send(Data::Text(text.clone())).is_ok() {
                        out.sent(&text);
                    }
                }
                None => {
                    lines = None;
                    deadline = Some(Instant::now() + wait);
                }
            },
            ev = events.recv() => match ev {
                Some(Event::Message(data)) => {
                    if enough(received) {
                        continue; // arrived while closing
                    }
                    out.received(&data);
                    received += 1;
                    if enough(received) {
                        sender.close();
                    }
                }
                Some(Event::Closed { code, reason }) => {
                    out.closed(code, &reason);
                    return Ok(i32::from(short(received)));
                }
                Some(Event::Error(e)) => {
                    out.error(&e);
                    return Ok(1);
                }
                None => return Ok(i32::from(short(received))),
            },
            _ = async { sleep.unwrap().await }, if deadline.is_some() => {
                deadline = None;
                if short(received) {
                    out.error(&format!("{received} of {} messages within {} ms", args.count.unwrap_or(0), wait.as_millis()));
                }
                sender.close();
            }
        }
    }
}
