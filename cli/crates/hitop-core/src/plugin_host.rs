//! Installed plugins and the sandboxed child process that runs one hook
//! (docs/PLUGINS_THREAT_MODEL.md), shared by the CLI and the desktop app.
//!
//! Plugins live in `<config>/hitop/plugins/`: `<name>.js` and `plugins.json`
//! (name, version, hooks, SHA-256 as installed). They are never part of a
//! collection, an export or sync. Each call runs in a new child process (the
//! caller's own executable with a marker argument: `hitop plugin-run`, the
//! desktop's `--hitop-plugin-run`) with an empty environment, the plugin's code
//! and the call's arguments on stdin and nothing else; it is ended after 5 s,
//! and on Linux and macOS its memory is capped.
#![cfg(feature = "native")]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::plugins::{self, Manifest, MAX_BYTES, TIME_LIMIT_MS};
use crate::storage;

/// The most a call's input (`{source, hook, args}` as JSON) may be: the plugin
/// (up to 1 MB) and its arguments, with room for JSON escaping. The parent
/// refuses a larger call; the child reads no more.
pub const INPUT_MAX: usize = 8 * 1024 * 1024;

/// One installed plugin, as `plugins.json` keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installed {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub sha256: String,
}

/// How to start the child: an executable and the arguments that make it run
/// [`child_main`].
#[derive(Debug, Clone)]
pub struct Child {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl Child {
    /// This process's own executable with `args`.
    pub fn current_exe(args: &[&str]) -> Result<Child> {
        Ok(Child { program: std::env::current_exe()?, args: args.iter().map(|a| a.to_string()).collect() })
    }
}

/// `<config>/hitop/plugins`, created when missing.
pub fn dir() -> Result<PathBuf> {
    let d = storage::config_dir()?.join("plugins");
    fs::create_dir_all(&d)?;
    Ok(d)
}

fn index_path(d: &Path) -> PathBuf {
    d.join("plugins.json")
}

/// The installed plugins, by name.
pub fn list() -> Result<Vec<Installed>> {
    list_in(&dir()?)
}

fn list_in(d: &Path) -> Result<Vec<Installed>> {
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

/// Lowercase hex SHA-256.
pub fn sha256(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The installed plugin called `name`, if any.
pub fn installed(name: &str) -> Result<Option<Installed>> {
    Ok(list()?.into_iter().find(|p| p.manifest.name == name))
}

/// Installs a plugin from its source, replacing one with the same name
/// (callers ask first: [`installed`]).
pub fn install(source: &str) -> Result<Installed> {
    let manifest = plugins::manifest(source).map_err(anyhow::Error::msg)?;
    let d = dir()?;
    let mut list = list_in(&d)?;
    let entry = Installed { manifest: manifest.clone(), sha256: sha256(source.as_bytes()) };
    let path = d.join(format!("{}.js", manifest.name));
    let tmp = d.join(format!("{}.js.tmp", manifest.name));
    fs::write(&tmp, source)?;
    fs::rename(&tmp, &path)?;
    list.retain(|p| p.manifest.name != manifest.name);
    list.push(entry.clone());
    list.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    save(&d, &list)?;
    Ok(entry)
}

/// Removes an installed plugin; false when none has that name.
pub fn remove(name: &str) -> Result<bool> {
    let d = dir()?;
    let mut list = list_in(&d)?;
    let before = list.len();
    list.retain(|p| p.manifest.name != name);
    if list.len() == before {
        return Ok(false);
    }
    save(&d, &list)?;
    let _ = fs::remove_file(d.join(format!("{name}.js")));
    Ok(true)
}

/// The desktop's plugin commands, by name, with their JSON arguments: `list`;
/// `install {source, force}` → `{installed}`, or `{exists, manifest}` when a
/// plugin of that name is installed and `force` is false; `remove {name}` →
/// true when one was removed; `call {name, hook, args}` → the answer, checked
/// by [`plugins::check_answer`].
pub fn command(child: &Child, install_hint: &str, cmd: &str, args: &Value) -> Result<Value, String> {
    let text = |k: &str| args[k].as_str().unwrap_or_default().to_string();
    let fail = |e: anyhow::Error| format!("{e:#}");
    match cmd {
        "list" => Ok(json!(list().map_err(fail)?)),
        "install" => {
            let source = text("source");
            let manifest = plugins::manifest(&source)?;
            if !args["force"].as_bool().unwrap_or(false) {
                if let Some(old) = installed(&manifest.name).map_err(fail)? {
                    return Ok(json!({ "exists": old, "manifest": manifest }));
                }
            }
            Ok(json!({ "installed": install(&source).map_err(fail)? }))
        }
        "remove" => Ok(json!(remove(&text("name")).map_err(fail)?)),
        "call" => {
            let name = text("name");
            let hook = text("hook");
            let call_args = args["args"].as_array().cloned().unwrap_or_default();
            // Decision 11: a value call's arguments are text, never converted.
            if hook == "value" && call_args.iter().any(|a| !a.is_string()) {
                return Err(format!("plugin '{name}': a value call's arguments are text"));
            }
            call_value(child, install_hint, &name, &hook, call_args)
        }
        _ => Err(format!("unknown plugin command '{cmd}'")),
    }
}

/// The plugin host: runs `hook` of the installed plugin `name` in the child,
/// after checking the file is the one that was installed. `install_hint` is how
/// the message tells a person to install it (the CLI's command, the
/// desktop's Settings page).
pub fn call(child: &Child, install_hint: &str, name: &str, hook: &str, args: &[String]) -> Result<String, String> {
    match call_value(child, install_hint, name, hook, args.iter().map(|a| Value::from(a.as_str())).collect())? {
        Value::String(s) => Ok(s),
        _ => Err(format!("plugin '{name}' answered something other than text")),
    }
}

/// [`call`] with any JSON arguments (a viewer's response, an importer's file,
/// a snippet's request); the answer is checked by [`plugins::check_answer`].
pub fn call_value(child: &Child, install_hint: &str, name: &str, hook: &str, args: Vec<Value>) -> Result<Value, String> {
    let fail = |e: anyhow::Error| format!("plugin '{name}': {e:#}");
    let d = dir().map_err(fail)?;
    let list = list_in(&d).map_err(fail)?;
    let Some(p) = list.into_iter().find(|p| p.manifest.name == name) else {
        return Err(format!("this request uses plugin '{name}', which isn't installed ({install_hint})"));
    };
    if !p.manifest.hooks.iter().any(|h| h == hook) {
        return Err(format!("plugin '{name}' has no {hook} hook"));
    }
    let source = fs::read(d.join(format!("{name}.js"))).map_err(|e| format!("plugin '{name}': {e}"))?;
    if sha256(&source) != p.sha256 {
        return Err(format!("plugin '{name}' changed since it was installed; check it and install it again ({install_hint})"));
    }
    let source = String::from_utf8(source).map_err(|_| format!("plugin '{name}' isn't UTF-8 text"))?;
    let answer = run_child(child, &source, hook, args).map_err(|e| format!("plugin '{name}' {e}"))?;
    plugins::check_answer(name, hook, &answer)
}

/// Runs one hook in the child: no environment, input on stdin, ended after
/// [`TIME_LIMIT_MS`].
fn run_child(child: &Child, source: &str, hook: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut command = Command::new(&child.program);
    command.args(&child.args).env_clear().stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let input = serde_json::to_vec(&json!({"source": source, "hook": hook, "args": args})).map_err(|e| e.to_string())?;
    if input.len() > INPUT_MAX {
        return Err(format!("wasn't called: its input would be more than {} MB", INPUT_MAX / (1024 * 1024)));
    }
    let mut process = command.spawn().map_err(|e| format!("can't start: {e}"))?;
    let mut stdin = process.stdin.take().expect("piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = process.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut out = vec![];
        let _ = (&mut stdout).take((MAX_BYTES * 2 + 4096) as u64).read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + Duration::from_millis(TIME_LIMIT_MS);
    let status = loop {
        match process.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = process.kill();
                let _ = process.wait();
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

/// The child's whole life: reads `{source, hook, args}` on stdin, prints
/// `{"ok": answer}` or `{"error": message}`, returns the exit code. Call it
/// before anything else in `main` when the marker argument is there.
pub fn child_main() -> i32 {
    limit_memory();
    crate::scripting::set_limits(10_000_000, 400);
    let mut input = vec![];
    if std::io::stdin().take(INPUT_MAX as u64).read_to_end(&mut input).is_err() {
        return 2;
    }
    let out = match serde_json::from_slice::<Value>(&input) {
        Ok(v) => {
            let source = v["source"].as_str().unwrap_or_default();
            let hook = v["hook"].as_str().unwrap_or_default();
            let args = v["args"].as_array().cloned().unwrap_or_default();
            match crate::scripting::run_plugin_hook(source, hook, &args) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 11: a value call's arguments are text; others are refused
    /// before any plugin is looked up or a child started.
    #[test]
    fn a_value_call_with_arguments_that_arent_text_is_refused() {
        let child = Child { program: PathBuf::from("/nonexistent/hitop"), args: vec![] };
        for args in [json!([1]), json!(["a", null]), json!([{"k": "v"}])] {
            let e = command(&child, "hint", "call", &json!({"name": "p", "hook": "value", "args": args})).unwrap_err();
            assert!(e.contains("a value call's arguments are text"), "{args}: {e}");
        }
    }
}
