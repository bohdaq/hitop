//! Self-update, only in the GitHub Release builds (Cargo feature `updater`, turned on by
//! release-desktop.yml). The official updater plugin checks the endpoint in
//! tauri.github.conf.json (`plugins.updater`: the latest GitHub Release's latest.json),
//! verifies the download against the public key there and installs it; the process plugin
//! relaunches. The web app drives both (webapp/src/platform/tauri-updater.js) through the
//! permissions in capabilities/updater/, which build.rs includes only with the feature.
//! The Mac App Store build (plain `tauri build`, no feature) contains none of this:
//! `updater_status` then answers an error and the web app shows nothing about updates.

use serde_json::{json, Value};

/// Whether this build updates itself: `Err` says why not.
pub struct Updates(Result<(), String>);

/// Sets up the plugins where this build can update itself. A failure here never stops
/// the app: it only means no updates (`updater_status` says why).
pub fn init(_app: &tauri::App) -> Updates {
    #[cfg(feature = "updater")]
    {
        Updates(start(_app))
    }
    #[cfg(not(feature = "updater"))]
    {
        Updates(Err("this build has no updater".into()))
    }
}

#[cfg(feature = "updater")]
fn start(app: &tauri::App) -> Result<(), String> {
    // The endpoint and key come with tauri.github.conf.json; the plugin can't start without them.
    if !app.config().plugins.0.contains_key("updater") {
        return Err("this build has no update endpoint".into());
    }
    // On Linux only the AppImage replaces itself: a .deb or .rpm install would be offered
    // the AppImage (latest.json's linux-x86_64), so its updates come with the package.
    #[cfg(target_os = "linux")]
    if !matches!(tauri::utils::platform::bundle_type(), Some(tauri::utils::config::BundleType::AppImage)) {
        return Err("only the AppImage updates itself; download new versions of this package from GitHub Releases".into());
    }
    let handle = app.handle();
    handle.plugin(tauri_plugin_updater::Builder::new().build()).map_err(|e| format!("the updater didn't start: {e}"))?;
    handle.plugin(tauri_plugin_process::init()).map_err(|e| format!("the updater didn't start: {e}"))?;
    Ok(())
}

/// `{version}` (the running version) when this build updates itself; an error otherwise.
#[tauri::command]
pub fn updater_status(app: tauri::AppHandle, state: tauri::State<'_, Updates>) -> Result<Value, String> {
    state.0.clone()?;
    Ok(json!({ "version": app.package_info().version.to_string() }))
}
