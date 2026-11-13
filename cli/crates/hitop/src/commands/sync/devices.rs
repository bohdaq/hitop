//! `hitop sync devices`: your devices (device keys) on the sync server: list, rename, remove.
//! A device that holds workspace keys is removed only with `--force`; its workspaces then rotate
//! their key (the server asks every member's devices to), and it is signed out.

use anyhow::{bail, Result};
use colored::Colorize;
use serde_json::json;

use super::client::api_error;
use super::runs::when;
use super::DevicesCmd;

pub fn run(cmd: DevicesCmd) -> Result<i32> {
    let s = super::session()?;
    match cmd {
        DevicesCmd::Ls => {
            let r = s.api.call("GET", "/v1/me/devices", None)?;
            let list = r["devices"].as_array().cloned().unwrap_or_default();
            for d in &list {
                let last = d["lastUsedAt"].as_i64().map(when).unwrap_or_else(|| "not seen yet".into());
                let this = if d["thisDevice"] == true || d["id"] == s.account.key_id.as_str() { " (this device)".green().to_string() } else { String::new() };
                let keys = match (d["workspaces"].as_i64().unwrap_or(0), d["onlyHere"].as_i64().unwrap_or(0)) {
                    (0, _) => String::new(),
                    (n, 0) => format!(", keys of {n} shared workspace{}", if n == 1 { "" } else { "s" }),
                    (n, o) => format!(", keys of {n} shared workspace{} ({o} only here)", if n == 1 { "" } else { "s" }),
                };
                println!("{}{this}\n  {}  added {}, last used {last}{keys}", d["name"].as_str().unwrap_or("").bold(), d["id"].as_str().unwrap_or("").dimmed(), d["createdAt"].as_i64().map(when).unwrap_or_default());
            }
            if let Some(max) = r["limit"].as_i64() {
                println!("{} of {max} devices on your plan", list.len());
            }
            Ok(0)
        }
        DevicesCmd::Rename { id, name } => {
            let r = s.api.call("PUT", &format!("/v1/me/keys/{id}"), Some(json!({ "name": name })))?;
            println!("Renamed to {}", r["name"].as_str().unwrap_or(&name));
            Ok(0)
        }
        DevicesCmd::Remove { id, force } => {
            if id == s.account.key_id {
                bail!("that is this device: run `hitop sync logout` here instead");
            }
            // `untied`: sign-ins of older clients (tied to no device) end too; never this one.
            let r = s.api.raw("DELETE", &format!("/v1/me/keys/{id}?untied=true{}", if force { "&force=true" } else { "" }), None)?;
            match r.status {
                200 | 204 => {
                    let rotated = r.json["rotationRequested"].as_array().map_or(0, Vec::len);
                    let ended = r.json["sessionsEnded"].as_i64().unwrap_or(0);
                    let untied = r.json["untiedSessionsEnded"].as_i64().unwrap_or(0);
                    println!("Removed {id}.");
                    if ended > 0 || untied > 0 {
                        println!("It is signed out.");
                    }
                    if untied > 0 {
                        println!("Older HITOP versions signed in to your account were signed out too ({untied}): sign in there again.");
                    }
                    if rotated > 0 {
                        println!("{rotated} workspace{} will rotate their key: the next sync on an owner's device does it.", if rotated == 1 { "" } else { "s" });
                    }
                    Ok(0)
                }
                409 if r.json["error"] == "key_in_use" => {
                    let n = r.json["workspaces"].as_i64().unwrap_or(0);
                    let only = r.json["onlyHere"].as_i64().unwrap_or(0);
                    let lose = if only > 0 { format!(" {only} of them only that device can open: their data is lost to you unless another member has it.") } else { String::new() };
                    bail!("that device holds the keys of {n} shared workspace{}.{lose} Run again with --force to remove it; those workspaces then rotate their key. If your recovery phrase or password unlock was set up there, set it up again on a device you keep", if n == 1 { "" } else { "s" })
                }
                _ => Err(api_error(&r)),
            }
        }
    }
}

