//! Sync: a collection imported from a spec, brought up to a newer spec.
//!
//! Each request's record holds a hash of every spec-owned field as last
//! imported (the base). A field the spec changed is updated when the
//! request still has the base value, and kept (listed as kept) when you
//! edited it. Names, files, scripts and examples you saved are never the
//! spec's. Operations gone from the spec are listed; they're removed only
//! with `prune`.
//!
//! Header and query parameter rows merge one by one (the record keeps a hash
//! per row): a row you added, edited or removed stays as you have it, and each
//! row the spec added, changed or removed is applied when you didn't touch it.
use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{build, parse, unique_file, Mapped, OpRecord, OpenApiReport, Options, SpecRecord, BASE_URL, EXAMPLE_SOURCE, RECORD};
use crate::format::names;
use crate::model::{Auth, Collection, Header, Param, Request};

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
    /// Header and query parameter rows, when `headers` or `params` merged
    /// row by row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<RowChange>,
}

/// One header or query parameter row in a row-by-row merge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    /// `headers` or `params`.
    pub field: String,
    /// The header's or parameter's name.
    pub name: String,
    /// `added`, `changed` or `removed`: what the spec did (outcomes `updated`
    /// and `kept`) or what you did (outcome `yours`).
    pub change: String,
    /// `updated`: the spec's change is applied; `kept`: the spec changed a
    /// row you had edited, yours stays; `yours`: your change to a row the spec
    /// didn't change, which stays.
    pub outcome: String,
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

/// The fields merged row by row.
const ROW_FIELDS: [&str; 2] = ["headers", "params"];

/// Each row's merge key: the header name in lower case or the parameter key
/// as written, `name#2` for the second row of one name; None for a row
/// without a name.
fn row_keys<T>(rows: &[T], kind: &RowKind<T>) -> Vec<Option<String>> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    rows.iter()
        .map(|r| {
            let n = (kind.name)(r);
            if n.is_empty() {
                return None;
            }
            let key = if kind.fold { n.to_lowercase() } else { n.to_string() };
            let count = seen.entry(key.clone()).or_insert(0);
            *count += 1;
            Some(if *count == 1 { key } else { format!("{key}#{count}") })
        })
        .collect()
}

/// How rows of one kind are named and hashed.
struct RowKind<T> {
    name: fn(&T) -> &str,
    /// Names match in any case (headers).
    fold: bool,
    hash: fn(&T) -> String,
}

const HEADER_ROWS: RowKind<Header> = RowKind { name: |h| &h.name, fold: true, hash: |h| hash(&json!([h.value, h.enabled])) };
const PARAM_ROWS: RowKind<Param> = RowKind { name: |p| &p.key, fold: false, hash: |p| hash(&json!(p.value)) };

fn row_hashes<T>(rows: &[T], kind: &RowKind<T>) -> BTreeMap<String, String> {
    row_keys(rows, kind).into_iter().zip(rows).filter_map(|(k, r)| Some((k?, (kind.hash)(r)))).collect()
}

/// The per-row base of a request as imported (`OpRecord::rows`).
pub(super) fn request_rows(r: &Request) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut rows = BTreeMap::new();
    let headers = row_hashes(&r.headers, &HEADER_ROWS);
    if !headers.is_empty() {
        rows.insert("headers".to_string(), headers);
    }
    let params = row_hashes(&r.disabled_params, &PARAM_ROWS);
    if !params.is_empty() {
        rows.insert("params".to_string(), params);
    }
    rows
}

/// Merges the rows of `spec` into `current` against `base` (key → hash),
/// row by row, listing each row in `out`. Returns (applied, kept): whether
/// a spec change was applied, and whether one was left out for your edit.
fn merge_rows<T: Clone>(field: &str, current: &mut Vec<T>, spec: &[T], base: &BTreeMap<String, String>, kind: &RowKind<T>, out: &mut Vec<RowChange>) -> (bool, bool) {
    let (name, row_hash) = (kind.name, kind.hash);
    let spec_keys = row_keys(spec, kind);
    let mut keys: Vec<String> = vec![];
    for k in spec_keys.iter().flatten().chain(row_keys(current, kind).iter().flatten()).chain(base.keys()) {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    let what = |from: &Option<String>, to: &Option<String>| match (from, to) {
        (None, _) => "added",
        (_, None) => "removed",
        _ => "changed",
    };
    let (mut applied, mut kept) = (false, false);
    for key in keys {
        let cur_keys = row_keys(current, kind);
        let ci = cur_keys.iter().position(|k| k.as_deref() == Some(key.as_str()));
        let si = spec_keys.iter().position(|k| k.as_deref() == Some(key.as_str()));
        let (b, n, c) = (base.get(&key).cloned(), si.map(|i| row_hash(&spec[i])), ci.map(|i| row_hash(&current[i])));
        let label = si.map(|i| name(&spec[i])).or_else(|| ci.map(|i| name(&current[i]))).unwrap_or(key.split('#').next().unwrap_or(&key)).to_string();
        let mut row = |change: &str, outcome: &str| out.push(RowChange { field: field.into(), name: label.clone(), change: change.into(), outcome: outcome.into() });
        if n == b {
            if c != b {
                row(what(&b, &c), "yours");
            }
            continue;
        }
        if c == n {
            continue;
        }
        if c != b {
            kept = true;
            row(what(&b, &n), "kept");
            continue;
        }
        match (ci, si) {
            (Some(i), Some(s)) => current[i] = spec[s].clone(),
            (Some(i), None) => {
                current.remove(i);
            }
            (None, Some(s)) => {
                // After the spec's row before it, where you have that one.
                let before = spec_keys[..s].iter().rev().flatten().find_map(|k| cur_keys.iter().position(|c| c.as_deref() == Some(k.as_str())));
                let at = before.map(|i| i + 1).unwrap_or(current.len());
                current.insert(at, spec[s].clone());
            }
            (None, None) => continue,
        }
        applied = true;
        row(what(&b, &n), "updated");
    }
    (applied, kept)
}

/// The base rows of `field`: the record's, or none at all when the base had
/// no rows (a record from before 1.1 has no row hashes otherwise: None).
fn base_rows(record: Option<&OpRecord>, base_hash: Option<&String>, field: &str) -> Option<BTreeMap<String, String>> {
    let record = record?;
    if let Some(rows) = record.rows.get(field) {
        return Some(rows.clone());
    }
    let empty = hash(&json!([]));
    (!record.rows.is_empty() || base_hash == Some(&empty)).then(BTreeMap::new)
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
                (_, true) => match base_rows(records[i].as_ref(), base.get(f), f).filter(|_| ROW_FIELDS.contains(&f)) {
                    Some(rows) => {
                        let (applied, kept) = if f == "headers" {
                            merge_rows(f, &mut current.headers, &op.request.headers, &rows, &HEADER_ROWS, &mut change.rows)
                        } else {
                            merge_rows(f, &mut current.disabled_params, &op.request.disabled_params, &rows, &PARAM_ROWS, &mut change.rows)
                        };
                        if applied {
                            change.updated.push(f.to_string());
                        }
                        if kept {
                            change.kept.push(f.to_string());
                        }
                    }
                    None => change.kept.push(f.to_string()),
                },
                _ => {}
            }
        }
        // The base is now this spec; renamed operations keep their new id.
        let mut rec = op.record.clone();
        rec.synced = request_hashes(&op.request);
        rec.rows = request_rows(&op.request);
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
