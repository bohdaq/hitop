// Security-scoped bookmarks for collection folders in the Mac App Store build.
//
// The App Sandbox lets the app into a folder only after the user picks it in
// the Open dialog, and only until the app quits. To open the same folder on
// the next launch, the app keeps a security-scoped bookmark of it (entitlement
// com.apple.security.files.bookmarks.app-scope, in Entitlements.mas.plist only)
// and resolves it before touching the folder.
//
// `access(path)` runs before every folder read, write and Git command: in the
// sandbox it resolves the folder's saved bookmark and starts accessing it once
// per launch, or, when there is none yet (the folder was just picked, so access
// is granted), saves one. Outside the sandbox (GitHub builds, other systems)
// it does nothing. Bookmarks live in <app data>/folder-bookmarks.json as
// base64; they hold no secrets, only the system's reference to the folder.
// A bookmark failure never fails the command: the read or write then reports
// the sandbox's own error, as before.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::{BTreeMap, HashSet};
    use std::path::PathBuf;
    use std::sync::Mutex;

    use base64::Engine;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSData, NSString, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL};
    use tauri::{AppHandle, Manager};

    /// Folders whose bookmark is resolved and being accessed in this launch.
    static ACTIVE: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    /// Serializes reads and writes of the bookmark file.
    static FILE: Mutex<()> = Mutex::new(());
    const STORE: &str = "folder-bookmarks.json";

    pub fn sandboxed() -> bool {
        std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()
    }

    fn store_path(app: &AppHandle) -> Option<PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join(STORE))
    }

    fn load(app: &AppHandle) -> BTreeMap<String, String> {
        store_path(app).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(app: &AppHandle, map: &BTreeMap<String, String>) -> Result<(), String> {
        let path = store_path(app).ok_or("no app data directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    }

    fn file_url(path: &str) -> Retained<NSURL> {
        NSURL::fileURLWithPath(&NSString::from_str(path))
    }

    /// A security-scoped bookmark of a folder the app can reach right now.
    fn create(url: &NSURL) -> Result<Vec<u8>, String> {
        url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(NSURLBookmarkCreationOptions::WithSecurityScope, None, None)
            .map(|data| data.to_vec())
            .map_err(|e| e.localizedDescription().to_string())
    }

    /// Resolves a saved bookmark: the URL and whether it is stale (the
    /// folder moved or was renamed; it still resolves, but should be saved again).
    fn resolve(bytes: &[u8]) -> Result<(Retained<NSURL>, bool), String> {
        let data = NSData::with_bytes(bytes);
        let mut stale = Bool::NO;
        // SAFETY: `stale` is a valid pointer for the duration of the call.
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(&data, NSURLBookmarkResolutionOptions::WithSecurityScope, None, &mut stale)
        }
        .map_err(|e| e.localizedDescription().to_string())?;
        Ok((url, stale.as_bool()))
    }

    fn remember(app: &AppHandle, path: &str, bytes: &[u8]) -> Result<(), String> {
        let _guard = FILE.lock().map_err(|e| e.to_string())?;
        let mut map = load(app);
        map.insert(path.to_string(), base64::engine::general_purpose::STANDARD.encode(bytes));
        save(app, &map)
    }

    pub fn access(app: &AppHandle, path: &str) {
        if !sandboxed() {
            return;
        }
        let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        let active = active.get_or_insert_with(HashSet::new);
        if active.contains(path) {
            return;
        }
        let saved = {
            let _guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
            load(app).get(path).and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        };
        match saved {
            Some(bytes) => match resolve(&bytes) {
                Ok((url, stale)) => {
                    // SAFETY: a plain Objective-C call on a valid NSURL. Access lasts
                    // until the app quits (the URL is kept alive below), which is
                    // what an open collection folder needs.
                    let started = unsafe { url.startAccessingSecurityScopedResource() };
                    if !started {
                        eprintln!("hitop: the sandbox refused the saved access to {path}; open the folder again");
                        return;
                    }
                    if stale {
                        if let Err(e) = create(&url).and_then(|b| remember(app, path, &b)) {
                            eprintln!("hitop: couldn't renew the bookmark for {path}: {e}");
                        }
                    }
                    // Keep the URL (and so the access) for the rest of this launch.
                    std::mem::forget(url);
                    active.insert(path.to_string());
                }
                Err(e) => eprintln!("hitop: the saved access to {path} no longer resolves ({e}); open the folder again"),
            },
            None => {
                // Just picked in the Open dialog (or never): save a bookmark if the
                // folder is reachable now, so the next launch can reopen it.
                match create(&file_url(path)).and_then(|b| remember(app, path, &b)) {
                    Ok(()) => {
                        active.insert(path.to_string());
                    }
                    Err(e) => eprintln!("hitop: no bookmark for {path}: {e}"),
                }
            }
        }
    }

    pub fn forget(app: &AppHandle, path: &str) -> Result<(), String> {
        let _guard = FILE.lock().map_err(|e| e.to_string())?;
        let mut map = load(app);
        if map.remove(path).is_some() {
            save(app, &map)?;
        }
        Ok(())
    }
}

/// Before the app reads or writes the folder at `path`: in the Mac App Store
/// sandbox, reopens it from its saved bookmark or saves one. A no-op elsewhere.
pub fn access(app: &AppHandle, path: &str) {
    #[cfg(target_os = "macos")]
    mac::access(app, path);
    #[cfg(not(target_os = "macos"))]
    let _ = (app, path);
}

/// Drops a folder's saved bookmark (the user closed the folder collection).
#[tauri::command(async)]
pub fn folder_forget(app: AppHandle, path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::forget(&app, &path);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, path);
        Ok(())
    }
}
