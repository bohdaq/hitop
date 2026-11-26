//! `hitop plugin install|ls|rm|call` and the `hitop plugin-run` child, over
//! hitop-core's `plugin_host` (docs/PLUGINS_THREAT_MODEL.md).
//!
//! Plugins live in `<config>/hitop/plugins/` (shared with the desktop app).
//! Each call runs in a new `hitop plugin-run` child process with an empty
//! environment and only the call's input; it is ended after 5 s, and on Linux
//! and macOS its memory is capped.
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use colored::Colorize;

use hitop_core::plugin_host::{self, Child};
use hitop_core::plugins;

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

pub fn run(cmd: PluginCmd) -> Result<()> {
    match cmd {
        PluginCmd::Install { file, force } => install(&file, force),
        PluginCmd::Ls { json } => {
            let list = plugin_host::list()?;
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
            if !plugin_host::remove(&name)? {
                bail!("no plugin named '{name}' is installed");
            }
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

fn install(file: &std::path::Path, force: bool) -> Result<()> {
    let bytes = fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let source = String::from_utf8(bytes).context("a plugin file is UTF-8 text")?;
    let manifest = plugins::manifest(&source).map_err(anyhow::Error::msg)?;
    if let Some(old) = plugin_host::installed(&manifest.name)? {
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
    let p = plugin_host::install(&source)?;
    let m = &p.manifest;
    println!("{} Installed plugin {} {} (hooks: {}; sha256 {})", "✓".green(), m.name.bold(), m.version, m.hooks.join(", "), &p.sha256[..12]);
    if m.hooks.iter().any(|h| h == "value") {
        println!("  Use it in a request as ${{plugin.{}(variable, \"text\")}}. It runs sandboxed, with only the arguments you pass.", m.name);
    }
    Ok(())
}

/// The plugin host: one hook of an installed plugin in a `hitop plugin-run` child.
pub fn host(name: &str, hook: &str, args: &[String]) -> std::result::Result<String, String> {
    let child = Child::current_exe(&["plugin-run"]).map_err(|e| format!("plugin '{name}' can't start: {e}"))?;
    plugin_host::call(&child, "hitop plugin install <file>", name, hook, args)
}

/// `hitop plugin-run` (hidden): the child that runs one hook.
pub fn child() -> i32 {
    plugin_host::child_main()
}
