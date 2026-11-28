//! Plugins in the desktop app (docs/PLUGINS_THREAT_MODEL.md): the same store
//! as the CLI (`<config>/hitop/plugins/`) and the same sandboxed child, which
//! is this app's own executable started with [`CHILD_ARG`]. The commands are
//! hitop-core's `plugin_host::command` (also behind the e2e mock's
//! `plugin-json` example).
use hitop_core::plugin_host::{self, Child};
use serde_json::{json, Value};

/// The argument that makes this executable the plugin child (lib.rs checks
/// it before anything else starts).
pub const CHILD_ARG: &str = "--hitop-plugin-run";

const INSTALL_HINT: &str = "Settings → Plugins";

fn child() -> Result<Child, String> {
    Child::current_exe(&[CHILD_ARG]).map_err(|e| format!("plugins can't start: {e}"))
}

fn command(cmd: &str, args: Value) -> Result<Value, String> {
    plugin_host::command(&child()?, INSTALL_HINT, cmd, &args)
}

/// Installs the plugin host, so hitop-core's interpolators can call plugins.
pub fn install_host() {
    hitop_core::plugins::install_host(Box::new(|name: &str, hook: &str, args: &[String]| {
        plugin_host::call(&child()?, INSTALL_HINT, name, hook, args)
    }));
}

/// `[{name, version, hooks, description?, sha256}]`.
#[tauri::command(async)]
pub fn plugins_list() -> Result<Value, String> {
    command("list", json!({}))
}

/// `{installed}`, or `{exists, manifest}` when one with that name is installed and not `force`.
#[tauri::command(async)]
pub fn plugin_install(source: String, force: bool) -> Result<Value, String> {
    command("install", json!({ "source": source, "force": force }))
}

#[tauri::command(async)]
pub fn plugin_remove(name: String) -> Result<Value, String> {
    command("remove", json!({ "name": name }))
}

/// One hook of an installed plugin in the child (5 s, no environment) → its
/// checked answer: text, or a viewer's table, or an importer's collections.
/// `value` takes text arguments only (hitop-core refuses others); the other
/// hooks get one structured input.
#[tauri::command(async)]
pub fn plugin_call(name: String, hook: String, args: Vec<Value>) -> Result<Value, String> {
    command("call", json!({ "name": name, "hook": hook, "args": args }))
}
