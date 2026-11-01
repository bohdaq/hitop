//! Sync: a collection imported from a spec, brought up to a newer spec.
//!
//! Each request's record holds a hash of every spec-owned field as last
//! imported (the base). A field the spec changed is updated when the
//! request still has the base value, and kept (listed as kept) when you
//! edited it. Names, files, scripts and examples you saved are never the
//! spec's. Operations gone from the spec are listed; they're removed only
//! with `prune`.
use std::collections::{BTreeMap, HashSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{build, parse, unique_file, Mapped, OpRecord, OpenApiReport, Options, SpecRecord, BASE_URL, EXAMPLE_SOURCE, RECORD};
use crate::format::names;
use crate::model::{Auth, Collection, Request};

/// The spec-owned fields of a request, in the order diffs list them.
pub const FIELDS: [&str; 7] = ["method", "url", "params", "headers", "body", "auth", "examples"];

/// One operation in a sync.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpChange {
    /// `METHOD /path`.
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// The request's name (yours, for one that exists).
    pub name: String,
    /// Its file in the folder, without `.yaml`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    /// Fields updated from the spec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<String>,
    /// Fields the spec changed that you had edited: yours stay.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept: Vec<String>,
}

/// Changes to the collection itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionChange {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables_added: Vec<String>,
}

/// What a sync does (or did).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDiff {
    pub added: Vec<OpChange>,
    pub changed: Vec<OpChange>,
    /// Operations no longer in the spec. Their requests stay unless pruned.
    pub removed: Vec<OpChange>,
    /// True when the removed ones were taken out.
    pub pruned: bool,
    /// Operations with nothing to update.
    pub unchanged: usize,
    pub collection: CollectionChange,
}

impl SyncDiff {
    /// True when the sync changes nothing.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.iter().all(|c| c.updated.is_empty())
            && (self.removed.is_empty() || !self.pruned)
            && self.collection.updated.is_empty()
            && self.collection.variables_added.is_empty()
    }
}

/// The collection after a sync, what changed, and the import report of the
/// new spec.
#[derive(Debug, Clone, PartialEq)]
pub struct Synced {
    pub collection: Collection,
    pub diff: SyncDiff,
    pub report: OpenApiReport,
}

/// A short stable hash of a JSON value (FNV-1a, 53 bits, hex).
pub fn hash(v: &Value) -> String {
    format!("{:x}", names::derived_id(&v.to_string()))
}

/// A spec-owned field of a request in a form that doesn't change when the
/// web app or a folder round trip saves it unchanged (blank header rows,
/// extra row keys, `{type: none}` without a collection auth).
fn field(r: &Request, name: &str) -> Value {
    match name {
        "method" => json!(r.method.to_uppercase()),
        "url" => json!(r.url),
        "params" => json!(r.disabled_params.iter().map(|p| [&p.key, &p.value]).collect::<Vec<_>>()),
        "headers" => json!(r
            .headers
            .iter()
            .filter(|h| !(h.name.is_empty() && h.value.is_empty()))
            .map(|h| json!([h.name, h.value, h.enabled]))
            .collect::<Vec<_>>()),
        "body" => json!({
            "body": r.body,
            "mode": r.body_mode,
            "fields": r.form_fields.iter().map(|f| json!([f.key, f.value, f.kind, f.enabled])).collect::<Vec<_>>(),
        }),
        "auth" => serde_json::to_value(&r.auth).unwrap_or(Value::Null),
        "examples" => Value::Array(spec_examples(r).cloned().collect()),
        _ => Value::Null,
    }
}

fn spec_examples(r: &Request) -> impl Iterator<Item = &Value> {
    r.examples.iter().filter(|e| e["source"] == EXAMPLE_SOURCE)
}

pub(super) fn request_hashes(r: &Request) -> BTreeMap<String, String> {
    FIELDS.iter().map(|f| (f.to_string(), hash(&field(r, f)))).collect()
}

/// Copies field `name` of `from` into `to`.
fn apply(to: &mut Request, from: &Request, name: &str) {
    match name {
        "method" => to.method = from.method.clone(),
        "url" => to.url = from.url.clone(),
        "params" => to.disabled_params = from.disabled_params.clone(),
        "headers" => to.headers = from.headers.clone(),
        "body" => {
            to.body = from.body.clone();
            to.body_mode = from.body_mode.clone();
            to.form_fields = from.form_fields.clone();
            to.body_type = from.body_type.clone();
        }
        "auth" => to.auth = from.auth.clone(),
        "examples" => {
            // The spec's examples, then the ones you saved.
            let mine: Vec<Value> = to.examples.iter().filter(|e| e["source"] != EXAMPLE_SOURCE).cloned().collect();
            to.examples = spec_examples(from).cloned().chain(mine).collect();
        }
        _ => {}
    }
}

fn record_of(r: &Request) -> Option<OpRecord> {
    serde_json::from_value(r.extra.get(RECORD)?.clone()).ok()
}

/// The three-way rule for one field: the spec's new value, the base (as
/// last imported) and yours. Returns (update, kept).
fn decide(base: Option<&String>, new: &str, current: &str) -> (bool, bool) {
    match base {
        Some(b) if b == new => (false, false),
        _ if current == new => (false, false),
        Some(b) if b == current => (true, false),
        // A record without this field's base: yours wins, as an edit.
        _ => (false, true),
    }
}

/// Syncs `existing` (imported from a spec before) with the spec in `text`.
pub fn sync(existing: &Collection, text: &str, opts: &Options, prune: bool) -> Result<Synced> {
    let Some(record) = existing.extra.get(RECORD) else {
        bail!("'{}' wasn't imported from an OpenAPI spec (its hitop.yaml has no `openapi` record); import the spec first", existing.name);
    };
    let old: SpecRecord = serde_json::from_value(record.clone()).unwrap_or_default();
    let opts = Options { flat: opts.flat || old.flat, source: opts.source.clone().or(old.source.clone()), ..opts.clone() };
    let spec = parse(text)?;
    let built = build(&spec, &opts)?;
    let mut report = built.report;
    let mut diff = SyncDiff { pruned: prune, ..Default::default() };
    let mut collection = existing.clone();

    // Existing requests by operationId, then by `METHOD /path`.
    let records: Vec<Option<OpRecord>> = existing.requests.iter().map(record_of).collect();
    let files = names::assign_files(&existing.requests.iter().map(|r| (r.name.as_str(), r.file.as_deref())).collect::<Vec<_>>())?;
    let mut used: HashSet<String> = files.iter().map(|f| f.to_lowercase()).collect();
    let mut matched = vec![false; existing.requests.len()];
    let find = |matched: &[bool], pred: &dyn Fn(&OpRecord) -> bool| records.iter().enumerate().position(|(i, r)| !matched[i] && r.as_ref().is_some_and(pred));

    for op in built.ops {
        let by_id = op.record.operation_id.as_ref().and_then(|id| find(&matched, &|r| r.operation_id.as_ref() == Some(id)));
        let at = by_id.or_else(|| find(&matched, &|r| r.operation == op.record.operation));
        let Some(i) = at else {
            let file = unique_file(&op.file, &mut used);
            let mut r = op.request;
            r.id = names::request_id(&file);
            r.extra.insert(RECORD.into(), serde_json::to_value(&op.record)?);
            diff.added.push(OpChange { operation: op.record.operation.clone(), operation_id: op.record.operation_id.clone(), name: r.name.clone(), file: file.clone(), ..Default::default() });
            r.file = Some(file);
            collection.requests.push(r);
            continue;
        };
        matched[i] = true;
        let base = records[i].clone().unwrap_or_default().synced;
        let current = &mut collection.requests[i];
        let mut change = OpChange { operation: op.record.operation.clone(), operation_id: op.record.operation_id.clone(), name: current.name.clone(), file: files[i].clone(), ..Default::default() };
        for f in FIELDS {
            let new = hash(&field(&op.request, f));
            let mine = hash(&field(current, f));
            match decide(base.get(f), &new, &mine) {
                (true, _) => {
                    apply(current, &op.request, f);
                    change.updated.push(f.to_string());
                }
                (_, true) => change.kept.push(f.to_string()),
                _ => {}
            }
        }
        // The base is now this spec; renamed operations keep their new id.
        let mut rec = op.record.clone();
        rec.synced = request_hashes(&op.request);
        current.extra.insert(RECORD.into(), serde_json::to_value(&rec)?);
        if change.updated.is_empty() && change.kept.is_empty() && records[i].as_ref().map(|r| (&r.operation, &r.operation_id)) == Some((&rec.operation, &rec.operation_id)) {
            diff.unchanged += 1;
        } else {
            diff.changed.push(change);
        }
    }

    // Requests from operations the spec no longer has.
    let mut keep = vec![true; existing.requests.len()];
    for (i, rec) in records.iter().enumerate() {
        let Some(rec) = rec else { continue };
        if matched[i] {
            continue;
        }
        let r = &existing.requests[i];
        diff.removed.push(OpChange { operation: rec.operation.clone(), operation_id: rec.operation_id.clone(), name: r.name.clone(), file: files[i].clone(), ..Default::default() });
        keep[i] = !prune;
    }
    let mut n = 0;
    collection.requests.retain(|_| {
        let k = n >= keep.len() || keep[n];
        n += 1;
        k
    });

    // The collection: baseUrl and auth by the same rule, new variables added.
    let new_base = built.variables.get(BASE_URL).cloned().unwrap_or_default();
    let cur_base = collection.variables.get(BASE_URL).cloned();
    match decide(old.synced.get(BASE_URL), &hash(&json!(new_base)), &hash(&json!(cur_base.clone().unwrap_or_default()))) {
        (true, _) => {
            collection.variables.insert(BASE_URL.into(), new_base.clone());
            diff.collection.updated.push(BASE_URL.into());
        }
        (_, true) => diff.collection.kept.push(BASE_URL.into()),
        _ => {}
    }
    if !opts.inline_auth {
        let new_auth = serde_json::to_value(&built.auth)?;
        let cur_auth = serde_json::to_value(&collection.auth)?;
        match decide(old.synced.get("auth"), &hash(&new_auth), &hash(&cur_auth)) {
            (true, _) => {
                collection.auth = built.auth.clone().filter(|a| *a != Auth::None);
                diff.collection.updated.push("auth".into());
            }
            (_, true) => diff.collection.kept.push("auth".into()),
            _ => {}
        }
    }
    for (k, v) in &built.variables {
        if !collection.variables.contains_key(k) {
            collection.variables.insert(k.clone(), v.clone());
            diff.collection.variables_added.push(k.clone());
        }
    }
    let new_record = SpecRecord { source: opts.source.clone(), ..built.record };
    if serde_json::to_value(&new_record)? != *record && diff.collection.updated.is_empty() && new_record.version != old.version {
        diff.collection.updated.push("version".into());
    }
    collection.extra.insert(RECORD.into(), serde_json::to_value(&new_record)?);

    report.mapped = Mapped { requests: collection.requests.iter().filter(|r| r.extra.contains_key(RECORD)).count(), ..report.mapped };
    Ok(Synced { collection, diff, report })
}
