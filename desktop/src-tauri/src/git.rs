// The Git panel's commands: a thin layer over hitop-git (cli/crates/hitop-git).
// `git {cmd, args}` runs one hitop-git command (hitop_git::json::call, the
// same entry point the web app's e2e mock uses); fetch, pull and push get the
// HTTPS token for the remote's host from the OS keychain (account
// git/<host>/token, service hitop). Tokens are never written to files.

use hitop_core::secrets;
use serde::Serialize;
use serde_json::Value;

fn path_of(args: &Value) -> Result<String, String> {
    args.get("path").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "argument path is missing".to_string())
}

fn host(path: &str) -> Option<String> {
    hitop_git::remote_host(path).ok().flatten()
}

fn auth_for(path: &str) -> hitop_git::Auth {
    let Some(host) = host(path) else { return hitop_git::Auth::default() };
    let store = secrets::os_store();
    hitop_git::Auth {
        token: store.get(&hitop_git::token_account(&host)).ok().flatten(),
        username: store.get(&hitop_git::username_account(&host)).ok().flatten(),
    }
}

#[tauri::command(async)]
pub fn git(cmd: String, args: Value) -> Result<Value, String> {
    let auth = if hitop_git::json::REMOTE_COMMANDS.contains(&cmd.as_str()) { auth_for(&path_of(&args)?) } else { hitop_git::Auth::default() };
    hitop_git::json::call(&cmd, &args, &auth).map_err(|e| e.0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    /// The remote's host; None for local and file:// remotes (no token needed).
    host: Option<String>,
    saved: bool,
    username: Option<String>,
}

#[tauri::command(async)]
pub fn git_token_status(path: String) -> Result<TokenStatus, String> {
    let Some(host) = host(&path) else { return Ok(TokenStatus { host: None, saved: false, username: None }) };
    let store = secrets::os_store();
    let saved = store.get(&hitop_git::token_account(&host)).map_err(|e| e.to_string())?.is_some();
    let username = store.get(&hitop_git::username_account(&host)).ok().flatten();
    Ok(TokenStatus { host: Some(host), saved, username })
}

#[tauri::command(async)]
pub fn git_token_set(path: String, token: String, username: Option<String>) -> Result<(), String> {
    let host = host(&path).ok_or("this remote is a local path: it needs no token")?;
    let token = token.trim();
    if token.is_empty() {
        return Err("paste a token".into());
    }
    let store = secrets::os_store();
    store.set(&hitop_git::token_account(&host), token).map_err(|e| e.to_string())?;
    match username.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => store.set(&hitop_git::username_account(&host), u).map_err(|e| e.to_string())?,
        None => {
            store.delete(&hitop_git::username_account(&host)).ok();
        }
    }
    Ok(())
}

#[tauri::command(async)]
pub fn git_token_delete(path: String) -> Result<bool, String> {
    let host = host(&path).ok_or("this remote is a local path: it has no token")?;
    let store = secrets::os_store();
    store.delete(&hitop_git::username_account(&host)).ok();
    store.delete(&hitop_git::token_account(&host)).map_err(|e| e.to_string())
}
