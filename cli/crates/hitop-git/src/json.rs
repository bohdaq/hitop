//! One entry point over JSON, what the desktop's `git` Tauri command calls
//! (and the web app's e2e mock, through `examples/git-json.rs`), so both
//! answer with the same shapes. `args.path` is the collection folder.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::{Auth, Error, Resolution, Result};

/// The commands that talk to a remote (they need [`Auth`]).
pub const REMOTE_COMMANDS: [&str; 3] = ["fetch", "pull", "push"];

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null)).map_err(|e| Error(format!("argument {key}: {e}")))
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| Error(e.to_string()))
}

/// Runs `cmd` with `args` ({ path, … }).
pub fn call(cmd: &str, args: &Value, auth: &Auth) -> Result<Value> {
    let path: String = arg(args, "path")?;
    match cmd {
        "status" => to_value(crate::status(&path)?),
        "diff" => to_value(crate::diff(&path, &arg::<String>(args, "file")?, arg::<Option<bool>>(args, "staged")?.unwrap_or(false))?),
        "stage" => crate::stage(&path, &arg::<Vec<String>>(args, "files")?).map(|_| Value::Null),
        "unstage" => crate::unstage(&path, &arg::<Vec<String>>(args, "files")?).map(|_| Value::Null),
        "discard" => crate::discard(&path, &arg::<Vec<String>>(args, "files")?).map(|_| Value::Null),
        "commit" => to_value(crate::commit(&path, &arg::<String>(args, "message")?)?),
        "setIdentity" => crate::set_identity(&path, &arg::<String>(args, "name")?, &arg::<String>(args, "email")?).map(|_| Value::Null),
        "branches" => to_value(crate::branches(&path)?),
        "createBranch" => crate::create_branch(&path, &arg::<String>(args, "name")?, arg::<Option<bool>>(args, "checkout")?.unwrap_or(true)).map(|_| Value::Null),
        "switchBranch" => crate::switch_branch(&path, &arg::<String>(args, "name")?).map(|_| Value::Null),
        "deleteBranch" => crate::delete_branch(&path, &arg::<String>(args, "name")?).map(|_| Value::Null),
        "mergeBranch" => to_value(crate::merge_branch(&path, &arg::<String>(args, "name")?)?),
        "conflict" => to_value(crate::conflict(&path, &arg::<String>(args, "file")?)?),
        "resolve" => crate::resolve(&path, &arg::<String>(args, "file")?, &arg::<Resolution>(args, "resolution")?).map(|_| Value::Null),
        "abortMerge" => crate::abort_merge(&path).map(|_| Value::Null),
        "remoteHost" => to_value(crate::remote_host(&path)?),
        "fetch" => crate::fetch(&path, auth).map(|_| Value::Null),
        "pull" => to_value(crate::pull(&path, auth)?),
        "push" => to_value(crate::push(&path, auth)?),
        _ => Err(Error(format!("unknown git command {cmd}"))),
    }
}

/// `call`'s answer as one JSON value: { ok } or { error }.
pub fn answer(cmd: &str, args: &Value, auth: &Auth) -> Value {
    match call(cmd, args, auth) {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e.0 }),
    }
}
