//! `plugin-json <cmd> <args JSON>`: runs one of the desktop's plugin commands
//! (`plugin_host::command`: list, install, remove, call) and prints
//! `{"ok": …}` or `{"error": …}`; started with `--hitop-plugin-run` it is the
//! plugin child, as the desktop app is. The web app's e2e mock of the desktop
//! (webapp/tests/tauri-mock.cjs) answers the `plugin*` Tauri commands with it.
//! Build: cargo build -p hitop-core --example plugin-json
use hitop_core::plugin_host::{self, Child};

fn main() {
    let mut argv = std::env::args().skip(1);
    let cmd = argv.next().unwrap_or_default();
    if cmd == "--hitop-plugin-run" {
        std::process::exit(plugin_host::child_main());
    }
    let args: serde_json::Value = serde_json::from_str(&argv.next().unwrap_or_else(|| "{}".into())).unwrap_or_default();
    let answer = Child::current_exe(&["--hitop-plugin-run"])
        .map_err(|e| e.to_string())
        .and_then(|child| plugin_host::command(&child, "Settings → Plugins", &cmd, &args));
    let out = match answer {
        Ok(v) => serde_json::json!({ "ok": v }),
        Err(e) => serde_json::json!({ "error": e }),
    };
    println!("{out}");
}
