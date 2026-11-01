//! hitop sync, the client side: which files to pull, push or show as a conflict. No I/O, no
//! cryptography and no native dependencies, so the CLI, the desktop and the web app (through
//! hitop-wasm) make the same decisions. The transport and the encryption
//! (`hitop-sync-crypto`) are the caller's.
//!
//! The unit of sync is a collection folder as files in memory ([`Files`]: `hitop.yaml`,
//! `requests/**`, `environments/*`, `globals.yaml`). Each file is one blob on the server, under a
//! random id; the blob's plaintext ([`BlobText`]) carries the path, so the server never sees a
//! file name. Secret values are never in these files (environment files mark secrets
//! `{secret: true}`), so they never sync; only their names do.
//!
//! [`State`] is what the last sync agreed on: per path the blob id, version, version vector and
//! the text (the *base* of a three-way merge). [`plan`] compares the local files and the remote
//! changes with it:
//!
//! - changed only here → push it as the next version;
//! - changed only there → write it here;
//! - changed on both sides → the same text is fine; `hitop.yaml`, environments and globals are
//!   merged key by key (two people adding requests both change `hitop.yaml`'s `order`); a request
//!   file changed on both sides, or a merge that touches the same key, is a [`Conflict`]. A
//!   conflicted path is never pushed until it is resolved ([`State::resolve`]: keep mine or take
//!   theirs).
//!
//! Two clients that create the same path at once make two blobs for it; the one with the smaller
//! blob id wins everywhere, and the client whose blob lost deletes it, merging its text first
//! (as a conflict when it differs).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::format::files::Files;
use crate::format::names;

pub type VersionVector = BTreeMap<String, u64>;

/// The longest path accepted from the server.
const MAX_PATH: usize = 512;

/// What the last sync agreed on for one path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub blob: String,
    pub version: u64,
    pub vv: VersionVector,
    /// The text both sides had; `None` once the file was deleted (the blob is a tombstone).
    #[serde(default)]
    pub base: Option<String>,
}

/// A path changed on both sides. The entry already has their version (so resolving needs no
/// server round trip); `base` is the text before either change and `theirs` their text
/// (`None`: they deleted it). The local file is mine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub theirs: Option<String>,
}

/// The client's sync state of one workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The server's change sequence number this state includes.
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub entries: BTreeMap<String, Entry>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, Conflict>,
    /// Blobs of ours that lost a same-path race (see the module docs) and must be deleted:
    /// blob id → (version, vv).
    #[serde(default)]
    pub losers: BTreeMap<String, (u64, VersionVector)>,
}

/// A blob from the server's change feed, decrypted. `path` and `text` come from [`BlobText`];
/// both are `None` for a deleted blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBlob {
    pub blob: String,
    pub version: u64,
    pub vv: VersionVector,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

/// A write to send: `text` `None` is a delete (tombstone). An empty `path` deletes a blob of ours
/// that lost a same-path race.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Push {
    pub path: String,
    pub blob: String,
    pub version: u64,
    pub vv: VersionVector,
    pub text: Option<String>,
}

/// What [`plan`] decided. Apply `write` and `remove` to the local folder, then send `push` in
/// order and record each accepted one with [`State::pushed`]; `state` already has the pulled
/// changes and the new conflicts. A 409 on a push means someone wrote meanwhile: sync again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub write: Files,
    pub remove: Vec<String>,
    pub push: Vec<Push>,
    /// Paths that are in conflict after this sync (new and older ones).
    pub conflicts: Vec<String>,
    /// Paths merged key by key (both sides changed different keys).
    pub merged: Vec<String>,
    /// Remote blobs that were ignored, with the reason (a path outside a collection folder).
    pub skipped: Vec<String>,
    pub state: State,
}

/// The plaintext of a blob: `{"v":1,"path":...,"text":...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobText {
    pub v: u32,
    pub path: String,
    pub text: String,
}

impl BlobText {
    pub fn encode(path: &str, text: &str) -> Vec<u8> {
        serde_json::to_vec(&BlobText { v: 1, path: path.to_string(), text: text.to_string() }).unwrap_or_default()
    }

    pub fn decode(bytes: &[u8]) -> Result<BlobText, String> {
        let b: BlobText = serde_json::from_slice(bytes).map_err(|e| format!("not a hitop sync file: {e}"))?;
        if b.v != 1 {
            return Err(format!("sync file format {} needs a newer hitop", b.v));
        }
        Ok(b)
    }
}

/// Whether `path` is a file of a collection folder that sync carries: `hitop.yaml`,
/// `globals.yaml`, `environments/<name>.yaml` and `requests/<path>.yaml` (or `.yml`), relative,
/// with no hidden, empty or `..` segment. Paths from the server are checked with it before
/// anything is written, so a member can't write outside the folder.
pub fn is_synced_path(path: &str) -> bool {
    if path.is_empty() || path.len() > MAX_PATH || path.contains('\\') || path.contains('\0') || path.starts_with('/') {
        return false;
    }
    if path.split('/').any(|s| s.is_empty() || s.starts_with('.') || s == "..") {
        return false;
    }
    if matches!(path, "hitop.yaml" | "globals.yaml" | "globals.yml") {
        return true;
    }
    let yaml = path.ends_with(".yaml") || path.ends_with(".yml");
    if let Some(rest) = path.strip_prefix("environments/") {
        return yaml && !rest.contains('/');
    }
    if let Some(rest) = path.strip_prefix("requests/") {
        let stem = rest.trim_end_matches(".yaml").trim_end_matches(".yml");
        return yaml && names::check_rel_path(stem).is_ok();
    }
    false
}

/// The files of `files` that sync carries ([`is_synced_path`]).
pub fn synced_files(files: &Files) -> Files {
    files.iter().filter(|(p, _)| is_synced_path(p)).map(|(p, t)| (p.clone(), t.clone())).collect()
}

fn bump(vv: &VersionVector, me: &str) -> VersionVector {
    let mut vv = vv.clone();
    *vv.entry(me.to_string()).or_insert(0) += 1;
    vv
}

/// Files merged key by key instead of as a whole.
fn structural(path: &str) -> bool {
    matches!(path, "hitop.yaml" | "globals.yaml" | "globals.yml") || path.starts_with("environments/")
}

impl State {
    /// Records a push the server accepted.
    pub fn pushed(&mut self, push: &Push) {
        if push.path.is_empty() {
            self.losers.remove(&push.blob);
            return;
        }
        self.entries.insert(push.path.clone(), Entry { blob: push.blob.clone(), version: push.version, vv: push.vv.clone(), base: push.text.clone() });
    }

    /// Resolves a conflict; `None` when the path isn't in conflict. Keeping mine leaves the local
    /// file as it is: the entry already has their version with their text as the base, so the
    /// next [`plan`] pushes the local file over it. Taking theirs returns what to write.
    pub fn resolve(&mut self, path: &str, keep_mine: bool) -> Option<Resolution> {
        let conflict = self.conflicts.remove(path)?;
        Some(match (keep_mine, conflict.theirs) {
            (true, _) => Resolution::KeepMine,
            (false, Some(text)) => Resolution::Write(text),
            (false, None) => Resolution::Remove,
        })
    }
}

/// What resolving a conflict asks of the local folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "action", content = "text")]
pub enum Resolution {
    KeepMine,
    Write(String),
    Remove,
}

/// Three-way merge of one YAML value: `Err` when both sides changed the same thing differently.
fn merge_value(base: Option<&yaml_serde::Value>, mine: Option<&yaml_serde::Value>, theirs: Option<&yaml_serde::Value>, key: &str) -> Result<Option<yaml_serde::Value>, ()> {
    use yaml_serde::Value;
    if mine == theirs {
        return Ok(mine.cloned());
    }
    if mine == base {
        return Ok(theirs.cloned());
    }
    if theirs == base {
        return Ok(mine.cloned());
    }
    match (base, mine, theirs) {
        (b, Some(Value::Mapping(m)), Some(Value::Mapping(t))) => {
            let empty = yaml_serde::Mapping::new();
            let b = match b {
                Some(Value::Mapping(b)) => b,
                None => &empty,
                Some(_) => return Err(()),
            };
            let mut out = yaml_serde::Mapping::new();
            let keys: Vec<&Value> = m.keys().chain(t.keys().filter(|k| !m.contains_key(*k))).chain(b.keys().filter(|k| !m.contains_key(*k) && !t.contains_key(*k))).collect();
            for k in keys {
                let name = k.as_str().unwrap_or("");
                if let Some(v) = merge_value(b.get(k), m.get(k), t.get(k), name)? {
                    out.insert(k.clone(), v);
                }
            }
            Ok(Some(Value::Mapping(out)))
        }
        (b, Some(Value::Sequence(m)), Some(Value::Sequence(t))) if key == "order" => {
            let empty = vec![];
            let b = match b {
                Some(Value::Sequence(b)) => b,
                None => &empty,
                Some(_) => return Err(()),
            };
            Ok(Some(Value::Sequence(merge_list(b, m, t))))
        }
        _ => Err(()),
    }
}

/// An ordered list (the request order): mine, without what they removed, plus what they added,
/// each placed after the item that precedes it in theirs.
fn merge_list(base: &[yaml_serde::Value], mine: &[yaml_serde::Value], theirs: &[yaml_serde::Value]) -> Vec<yaml_serde::Value> {
    let removed: Vec<&yaml_serde::Value> = base.iter().filter(|v| !theirs.contains(v)).collect();
    let mut out: Vec<yaml_serde::Value> = mine.iter().filter(|v| !removed.contains(v)).cloned().collect();
    for (i, v) in theirs.iter().enumerate() {
        if base.contains(v) || out.contains(v) {
            continue;
        }
        let at = theirs[..i].iter().rev().find_map(|prev| out.iter().position(|o| o == prev)).map_or(0, |p| p + 1);
        out.insert(at, v.clone());
    }
    out
}

/// Merges a structural file (`hitop.yaml`, an environment, globals) key by key. `None` when the
/// same key changed on both sides, or a text isn't a YAML mapping.
pub fn merge_text(path: &str, base: Option<&str>, mine: Option<&str>, theirs: Option<&str>) -> Option<Option<String>> {
    if !structural(path) {
        return None;
    }
    // A deleted file on one side and an edit on the other is a conflict, not a merge.
    let (mine, theirs) = (mine?, theirs?);
    let parse = |t: &str| yaml_serde::from_str::<yaml_serde::Value>(t).ok().filter(|v| v.is_mapping());
    let b = match base {
        Some(t) => Some(parse(t)?),
        None => None,
    };
    let (m, t) = (parse(mine)?, parse(theirs)?);
    let merged = merge_value(b.as_ref(), Some(&m), Some(&t), "").ok()??;
    if merged == m {
        return Some(Some(mine.to_string()));
    }
    if merged == t {
        return Some(Some(theirs.to_string()));
    }
    yaml_serde::to_string(&merged).ok().map(Some)
}

/// Decides one sync. `local` is the folder's synced files now ([`synced_files`]); `remote` the
/// decrypted changes since `state.seq` (the caller sets the new `seq` on the result's state);
/// `me` this device's id in version vectors; `new_blob_id` makes random blob ids.
pub fn plan(local: &Files, state: &State, remote: &[RemoteBlob], me: &str, new_blob_id: &mut dyn FnMut() -> String) -> Plan {
    let mut st = state.clone();
    let mut out = Plan::default();
    // Their latest change per path: (meta, text).
    let mut theirs: BTreeMap<String, RemoteBlob> = BTreeMap::new();
    let path_of = |st: &State, blob: &str| st.entries.iter().find(|(_, e)| e.blob == blob).map(|(p, _)| p.clone());

    for r in remote {
        // A tombstone for one of our losers: done.
        if st.losers.contains_key(&r.blob) {
            if r.deleted {
                st.losers.remove(&r.blob);
            }
            continue;
        }
        let known = path_of(&st, &r.blob);
        let path = if r.deleted {
            match known {
                Some(p) => p,
                None => continue, // never had it
            }
        } else {
            let Some(p) = r.path.clone().filter(|p| is_synced_path(p)) else {
                out.skipped.push(format!("blob {}: path {:?} is not a collection file", r.blob, r.path.as_deref().unwrap_or("")));
                continue;
            };
            if known.as_ref().is_some_and(|old| *old != p) {
                // hitop never moves a blob to another path (a rename is a delete and a new file).
                out.skipped.push(format!("blob {}: moved to another path", r.blob));
                continue;
            }
            p
        };
        if let Some(entry) = st.entries.get(&path) {
            if entry.blob == r.blob && r.version <= entry.version {
                // Already have it (our own push coming back, or an older page).
                continue;
            }
            if entry.blob != r.blob {
                if r.deleted {
                    continue;
                }
                if r.blob.as_str() > entry.blob.as_str() {
                    // Their blob lost the race; its writer merges and deletes it.
                    continue;
                }
                // Ours lost: delete it, and merge with theirs as if neither had a base.
                if entry.version > 0 {
                    st.losers.insert(entry.blob.clone(), (entry.version, entry.vv.clone()));
                }
                st.conflicts.remove(&path);
                st.entries.insert(path.clone(), Entry { blob: r.blob.clone(), version: 0, vv: VersionVector::new(), base: None });
            }
        }
        theirs.insert(path, r.clone());
    }

    for (path, r) in &theirs {
        let entry = st.entries.get(path).cloned();
        let base = entry.as_ref().and_then(|e| e.base.clone());
        let mine = local.get(path).cloned();
        let their_text = if r.deleted { None } else { r.text.clone() };
        let changed_here = mine != base;
        let new_entry = Entry { blob: r.blob.clone(), version: r.version, vv: r.vv.clone(), base: their_text.clone() };
        if let Some(c) = st.conflicts.get_mut(path) {
            // Still in conflict: track their newest text.
            c.theirs = their_text.clone();
            st.entries.insert(path.clone(), new_entry);
            continue;
        }
        if !changed_here || mine == their_text {
            match &their_text {
                Some(t) if mine.as_ref() != Some(t) => {
                    out.write.insert(path.clone(), t.clone());
                }
                None if mine.is_some() => out.remove.push(path.clone()),
                _ => {}
            }
            st.entries.insert(path.clone(), new_entry);
            continue;
        }
        if their_text == base {
            // Rewritten with the same text (re-encrypted after a key rotation): mine stays and
            // is pushed over their version.
            st.entries.insert(path.clone(), new_entry);
            continue;
        }
        match merge_text(path, base.as_deref(), mine.as_deref(), their_text.as_deref()) {
            Some(Some(merged)) => {
                if Some(&merged) != mine.as_ref() {
                    out.write.insert(path.clone(), merged);
                }
                out.merged.push(path.clone());
                st.entries.insert(path.clone(), new_entry);
            }
            _ => {
                st.conflicts.insert(path.clone(), Conflict { base, theirs: their_text });
                st.entries.insert(path.clone(), new_entry);
            }
        }
    }

    // The folder after the writes above, and what is left to push.
    let mut after = local.clone();
    for (p, t) in &out.write {
        after.insert(p.clone(), t.clone());
    }
    for p in &out.remove {
        after.remove(p);
    }
    let paths: BTreeSet<String> = after.keys().chain(st.entries.keys()).cloned().collect();
    for path in paths {
        if st.conflicts.contains_key(&path) || !is_synced_path(&path) {
            continue;
        }
        let now = after.get(&path).cloned();
        match st.entries.get(&path) {
            Some(e) if e.base == now => {}
            Some(e) => out.push.push(Push { path: path.clone(), blob: e.blob.clone(), version: e.version + 1, vv: bump(&e.vv, me), text: now }),
            None => {
                if let Some(text) = now {
                    out.push.push(Push { path: path.clone(), blob: new_blob_id(), version: 1, vv: bump(&VersionVector::new(), me), text: Some(text) });
                }
            }
        }
    }
    for (blob, (version, vv)) in &st.losers {
        out.push.push(Push { path: String::new(), blob: blob.clone(), version: version + 1, vv: bump(vv, me), text: None });
    }
    out.conflicts = st.conflicts.keys().cloned().collect();
    out.state = st;
    out
}

#[cfg(test)]
mod tests;
