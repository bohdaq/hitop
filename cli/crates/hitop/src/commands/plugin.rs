//! `hitop plugin install|ls|rm|call`, the plugin host and the `hitop plugin-run`
//! child process that runs one hook (docs/PLUGINS_THREAT_MODEL.md).
//!
//! Plugins live in `<config>/hitop/plugins/`: `<name>.js` and `plugins.json`
//! (name, version, hooks, SHA-256 as installed). They are never part of a
//! collection, an export or sync. Each call runs in a new child process of
//! this binary with an empty environment, the plugin's code and the call's
//! arguments on stdin and nothing else; it is ended after 5 s, and on Linux
//! and macOS its memory is capped.
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use hitop_core::plugins::{self, Manifest, MAX_BYTES, TIME_LIMIT_MS};
use hitop_core::storage;

#[derive(Subcommand)]
pub enum PluginCmd {
    /// Install a plugin from a file (it runs sandboxed: no network, files, environment or secrets)
    Install {
        /// The plugin's .js file
        file: PathBuf,
        /// Replace an installed plugin with the same name
        #[arg(long)]
        force: bool,
    },
    /// List the installed plugins
    Ls {
        /// As JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove an installed plugin
    Rm { name: String },
    /// Call a plugin's value hook with text arguments and print its answer
    Call { name: String, args: Vec<String> },
}

/// One installed plugin, as `plugins.json` keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub sha256: String,
}

fn dir() -> Result<PathBuf> {
    let d = storage::config_dir()?.join("plugins");
    fs::create_dir_all(&d)?;
    Ok(d)
}

fn index_path(d: &Path) -> PathBuf {
    d.join("plugins.json")
}

fn load(d: &Path) -> Result<Vec<Installed>> {
    match fs::read_to_string(index_path(d)) {
        Ok(text) => serde_json::from_str(&text).with_context(|| format!("reading {}", index_path(d).display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e).with_context(|| format!("reading {}", index_path(d).display())),
    }
}

fn save(d: &Path, list: &[Installed]) -> Result<()> {
    let tmp = d.join("plugins.json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(list)? + "\n")?;
    fs::rename(&tmp, index_path(d))?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn run(cmd: PluginCmd) -> Result<()> {
    let d = dir()?;
    match cmd {
        PluginCmd::Install { file, force } => install(&d, &file, force),
        PluginCmd::Ls { json } => {
            let list = load(&d)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else if list.is_empty() {
                println!("No plugins installed. `hitop plugin install <file>` adds one.");
            } else {
                for p in list {
                    let m = &p.manifest;
                    println!("{} {}  hooks: {}  sha256: {}", m.name.bold(), m.version, m.hooks.join(", "), &p.sha256[..12]);
                    if !m.description.is_empty() {
                        println!("  {}", m.description);
                    }
                }
            }
            Ok(())
        }
        PluginCmd::Rm { name } => {
            let mut list = load(&d)?;
            let before = list.len();
            list.retain(|p| p.manifest.name != name);
            if list.len() == before {
                bail!("no plugin named '{name}' is installed");
            }
            save(&d, &list)?;
            let _ = fs::remove_file(d.join(format!("{name}.js")));
            println!("{} Removed plugin {name}", "✓".green());
            Ok(())
        }
        PluginCmd::Call { name, args } => {
            let answer = host(&name, "value", &args).map_err(anyhow::Error::msg)?;
            println!("{answer}");
            Ok(())
        }
    }
}

fn install(d: &Path, file: &Path, force: bool) -> Result<()> {
    let bytes = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let source = String::from_utf8(bytes).context("a plugin file is UTF-8 text")?;
    let manifest = plugins::manifest(&source).map_err(anyhow::Error::msg)?;
    let mut list = load(d)?;
    if let Some(old) = list.iter().find(|p| p.manifest.name == manifest.name) {
        if !force {
            bail!(
                "a plugin named '{}' is installed already (version {}, hooks: {}); this file is version {} (hooks: {}). --force replaces it",
                manifest.name,
                old.manifest.version,
                old.manifest.hooks.join(", "),
                manifest.version,
                manifest.hooks.join(", ")
            );
        }
    }
    let hash = sha256(source.as_bytes());
    let path = d.join(format!("{}.js", manifest.name));
    let tmp = d.join(format!("{}.js.tmp", manifest.name));
    fs::write(&tmp, &source)?;
    fs::rename(&tmp, &path)?;
    list.retain(|p| p.manifest.name != manifest.name);
    list.push(Installed { manifest: manifest.clone(), sha256: hash.clone() });
    list.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    save(d, &list)?;
    println!(
        "{} Installed plugin {} {} (hooks: {}; sha256 {})",
        "✓".green(),
        manifest.name.bold(),
        manifest.version,
        manifest.hooks.join(", "),
        &hash[..12]
    );
    if manifest.hooks.iter().any(|h| h == "value") {
        println!("  Use it in a request as ${{plugin.{}(variable, \"text\")}}. It runs sandboxed, with only the arguments you pass.", manifest.name);
    }
    Ok(())
}

/// The plugin host: runs `hook` of the installed plugin `name` in a child
/// process, after checking the file is the one that was installed.
pub fn host(name: &str, hook: &str, args: &[String]) -> std::result::Result<String, String> {
    let fail = |e: anyhow::Error| format!("plugin '{name}': {e:#}");
    let d = dir().map_err(fail)?;
    let list = load(&d).map_err(fail)?;
    let Some(p) = list.into_iter().find(|p| p.manifest.name == name) else {
        return Err(format!("this request uses plugin '{name}', which isn't installed (hitop plugin install <file>)"));
    };
    if !p.manifest.hooks.iter().any(|h| h == hook) {
        return Err(format!("plugin '{name}' has no {hook} hook"));
    }
    let source = fs::read(d.join(format!("{name}.js"))).map_err(|e| format!("plugin '{name}': {e}"))?;
    if sha256(&source) != p.sha256 {
        return Err(format!("plugin '{name}' changed since it was installed; check it and install it again (hitop plugin install <file> --force)"));
    }
    let source = String::from_utf8(source).map_err(|_| format!("plugin '{name}' isn't UTF-8 text"))?;
    let answer = run_child(&source, hook, args.iter().map(|a| Value::from(a.as_str())).collect()).map_err(|e| format!("plugin '{name}' {e}"))?;
    match answer {
        Value::String(s) => Ok(s),
        _ => Err(format!("plugin '{name}' answered something other than text")),
    }
}

/// Runs one hook in `hitop plugin-run`: no environment, input on stdin,
/// ended after [`TIME_LIMIT_MS`].
fn run_child(source: &str, hook: &str, args: Vec<Value>) -> std::result::Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't start: {e}"))?;
    let mut child = Command::new(exe)
        .arg("plugin-run")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("can't start: {e}"))?;
    let input = serde_json::to_vec(&json!({"source": source, "hook": hook, "args": args})).map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().expect("piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = vec![];
        let _ = (&mut stdout).take((MAX_BYTES * 2 + 4096) as u64).read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + Duration::from_millis(TIME_LIMIT_MS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("was stopped: it ran longer than {} s", TIME_LIMIT_MS / 1000));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(format!("failed: {e}")),
        }
    };
    let _ = writer.join();
    let out = reader.join().unwrap_or_default();
    let answer: Value = match serde_json::from_slice(&out) {
        Ok(v) => v,
        Err(_) if !status.success() => return Err("stopped: it used too much memory or crashed".into()),
        Err(_) => return Err("answered nothing hitop can read".into()),
    };
    match (answer.get("ok"), answer.get("error")) {
        (Some(v), _) => Ok(v.clone()),
        (None, Some(Value::String(e))) => Err(format!("failed: {e}")),
        _ => Err("answered nothing hitop can read".into()),
    }
}

/// `hitop plugin-run` (hidden): the child that runs one hook. Reads
/// `{source, hook, args}` on stdin, prints `{"ok": answer}` or
/// `{"error": message}`.
pub fn child() -> i32 {
    limit_memory();
    hitop_core::scripting::set_limits(10_000_000, 400);
    let mut input = vec![];
    if std::io::stdin().take((MAX_BYTES * 3) as u64).read_to_end(&mut input).is_err() {
        return 2;
    }
    let out = match serde_json::from_slice::<Value>(&input) {
        Ok(v) => {
            let source = v["source"].as_str().unwrap_or_default();
            let hook = v["hook"].as_str().unwrap_or_default();
            let args = v["args"].as_array().cloned().unwrap_or_default();
            match hitop_core::scripting::run_plugin_hook(source, hook, &args) {
                Ok(answer) => json!({"ok": answer}),
                Err(e) => json!({"error": e.to_string()}),
            }
        }
        Err(e) => json!({"error": format!("bad input: {e}")}),
    };
    let _ = std::io::stdout().write_all(out.to_string().as_bytes());
    let _ = std::io::stdout().flush();
    0
}

/// Caps the child's memory (address space on Linux, data size on macOS) and
/// CPU time (a second stop behind the parent's 5 s).
#[cfg(unix)]
fn limit_memory() {
    const BYTES: libc::rlim_t = 1024 * 1024 * 1024;
    let set = |what, value: libc::rlim_t| {
        let l = libc::rlimit { rlim_cur: value, rlim_max: value };
        // SAFETY: setrlimit reads the struct; lowering our own limits only.
        unsafe { libc::setrlimit(what, &l) };
    };
    #[cfg(target_os = "linux")]
    set(libc::RLIMIT_AS, BYTES);
    #[cfg(not(target_os = "linux"))]
    set(libc::RLIMIT_DATA, BYTES);
    set(libc::RLIMIT_CPU, 10);
}

/// No memory cap on Windows yet (a Job object): the parent's time limit
/// still ends the child (docs/PLUGINS_THREAT_MODEL.md, residual risks).
#[cfg(not(unix))]
fn limit_memory() {}
