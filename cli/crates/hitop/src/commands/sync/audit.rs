//! `hitop sync audit`: a workspace's audit log (owners), or your own account's (`--mine`).
//! `--verify` recomputes the hash chain here, from the rows the server sends, instead of taking
//! the server's word for it, and remembers the head it saw (`audit-heads.json`): a log rewritten
//! later from that row on is caught the next time. The row hash is the server's
//! (sync-server/src/audit.rs, THREAT_MODEL.md "Audit log").

use anyhow::{bail, Result};
use colored::Colorize;
use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::client::Api;
use super::store;

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub struct Filter {
    pub event: Option<String>,
    pub actor: Option<String>,
    pub limit: usize,
}

/// The hash of one row, as the server computes it.
pub fn row_hash(prev: &str, workspace: &str, row: &Value) -> String {
    let s = |k: &str| row[k].as_str().unwrap_or("").to_string();
    // The hash covers `detail` exactly as the server stored it, which it sends as `detailJson`.
    let detail = row["detailJson"].as_str().map(str::to_string).unwrap_or_else(|| row["detail"].to_string());
    let fields = [
        prev.to_string(),
        workspace.to_string(),
        row["seq"].as_i64().unwrap_or(0).to_string(),
        row["at"].as_i64().unwrap_or(0).to_string(),
        s("event"),
        row["actor"]["id"].as_str().unwrap_or("").to_string(),
        row["actor"]["email"].as_str().unwrap_or("").to_string(),
        s("target"),
        detail,
        s("ip"),
        s("userAgent"),
    ];
    let mut h = Sha256::new();
    h.update(b"hitop-sync/v1/audit\0");
    for f in &fields {
        h.update((f.len() as u32).to_be_bytes());
        h.update(f.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn query(f: &Filter, before: Option<i64>, limit: usize) -> String {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.append_pair("limit", &limit.to_string());
    if let Some(b) = before {
        q.append_pair("before", &b.to_string());
    }
    if let Some(e) = &f.event {
        q.append_pair("event", e);
    }
    if let Some(a) = &f.actor {
        q.append_pair("actor", a);
    }
    q.finish()
}

/// Rows newest first, up to `filter.limit` (0 = all), and the last page's answer (head, anchor).
fn fetch(api: &Api, path: &str, filter: &Filter) -> Result<(Vec<Value>, Value)> {
    let mut rows = Vec::new();
    let mut before = None;
    loop {
        let want = if filter.limit == 0 { 500 } else { (filter.limit - rows.len()).min(500) };
        let page = api.call("GET", &format!("{path}?{}", query(filter, before, want)), None)?;
        rows.extend(page["events"].as_array().cloned().unwrap_or_default());
        before = page["next"].as_i64();
        if before.is_none() || (filter.limit != 0 && rows.len() >= filter.limit) {
            return Ok((rows, page));
        }
    }
}

fn time(at: i64) -> String {
    hitop_core::report::rfc3339(std::time::UNIX_EPOCH + std::time::Duration::from_secs(at.max(0) as u64))
}

fn print(rows: &[Value]) {
    if rows.is_empty() {
        println!("No events.");
        return;
    }
    for r in rows {
        let detail = match &r["detail"] {
            Value::Object(m) if m.is_empty() => String::new(),
            d => d.to_string(),
        };
        let who = r["actor"]["email"].as_str().filter(|s| !s.is_empty()).unwrap_or("-");
        let target = r["target"].as_str().filter(|s| !s.is_empty()).map(|t| format!(" → {t}")).unwrap_or_default();
        let ip = r["ip"].as_str().filter(|s| !s.is_empty()).map(|ip| format!(" from {ip}")).unwrap_or_default();
        println!("{:>5} {} {:<18} {who}{target}{ip} {}", r["seq"], time(r["at"].as_i64().unwrap_or(0)), r["event"].as_str().unwrap_or("").bold(), detail.dimmed());
    }
}

/// Recomputes the chain from the full log and checks it against the server's head and anchor,
/// the server's own check, and the head this computer saw last time. `Err` on any break.
fn verify(api: &Api, workspace: &str) -> Result<()> {
    let path = format!("/v1/workspaces/{workspace}/audit");
    let (mut rows, last) = fetch(api, &path, &Filter { event: None, actor: None, limit: 0 })?;
    rows.reverse();
    let anchor_seq = last["anchor"]["seq"].as_i64().unwrap_or(0);
    let (head_seq, head_hash) = (last["head"]["seq"].as_i64().unwrap_or(0), last["head"]["hash"].as_str().unwrap_or("").to_string());
    let mut prev = last["anchor"]["hash"].as_str().unwrap_or(GENESIS).to_string();
    let mut expect = anchor_seq + 1;
    for r in &rows {
        let seq = r["seq"].as_i64().unwrap_or(0);
        if seq != expect {
            bail!("audit log: row {expect} is missing (the server sent {seq} next)");
        }
        if r["prevHash"].as_str() != Some(prev.as_str()) {
            bail!("audit log: row {seq} doesn't follow the row before it");
        }
        let hash = row_hash(&prev, workspace, r);
        if r["hash"].as_str() != Some(hash.as_str()) {
            bail!("audit log: row {seq} was changed (its hash doesn't match its contents)");
        }
        prev = hash;
        expect = seq + 1;
    }
    if expect - 1 != head_seq || prev != head_hash {
        bail!("audit log: the chain doesn't end at the head the server reports ({head_seq})");
    }
    let server = api.call("GET", &format!("{path}/verify"), None)?;
    if server["ok"] != true {
        bail!("audit log: the server's own check failed: {}", server["problem"].as_str().unwrap_or("?"));
    }
    let mut heads = store::load_audit_heads()?;
    if let Some((seq, hash)) = heads.get(workspace) {
        if *seq > anchor_seq {
            let seen = rows.iter().find(|r| r["seq"].as_i64() == Some(*seq));
            if seen.and_then(|r| r["hash"].as_str()) != Some(hash.as_str()) {
                bail!("audit log: row {seq} isn't the one this computer verified before: the log was rewritten");
            }
        }
    }
    if head_seq > 0 {
        heads.insert(workspace.to_string(), (head_seq, head_hash.clone()));
        store::save_audit_heads(&heads)?;
    }
    println!("{} audit log intact: {} rows checked here (anchor {anchor_seq}, head {head_seq} {})", "✓".green(), rows.len(), &head_hash[..head_hash.len().min(16)]);
    Ok(())
}

pub fn run(api: &Api, workspace: Option<&str>, filter: Filter, json: bool, check: bool) -> Result<()> {
    let path = match workspace {
        Some(ws) => format!("/v1/workspaces/{ws}/audit"),
        None => "/v1/me/audit".to_string(),
    };
    let (rows, _) = fetch(api, &path, &filter)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        print(&rows);
    }
    if check {
        match workspace {
            Some(ws) => verify(api, ws)?,
            None => bail!("--verify checks a workspace's log (leave out --mine)"),
        }
    }
    Ok(())
}

// --- signed org exports and daily anchors (HITOP Cloud C5) -------------------------------------
//
// The server (sync-server/src/anchors.rs) exports an org's chains as JSON lines with an Ed25519
// signature line last, and mails its owners a signed anchor of the chain heads once a day. A log
// rewritten after an anchor left the server no longer has the anchored row at that seq.

/// The label the export's signature covers before the SHA-256 of the file.
pub const EXPORT_LABEL: &[u8] = b"hitop-sync/v1/audit-export\0";
pub const ANCHOR_HEADER: &str = "hitop-sync/v1/audit-anchor";
const BEGIN: &str = "-----BEGIN HITOP AUDIT ANCHOR-----";
const END: &str = "-----END HITOP AUDIT ANCHOR-----";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The first 16 hex characters of SHA-256 of the public key.
pub fn key_id(public: &[u8]) -> String {
    hex(&Sha256::digest(public))[..16].to_string()
}

/// The text an anchor's signature covers (the server's `anchors::anchor_text`).
pub fn anchor_text(server: &str, org: &str, day: &str, at: i64, chains: &[(String, i64, String)]) -> String {
    let mut t = format!("{ANCHOR_HEADER}\nserver {server}\norg {org}\nday {day}\nat {at}\n");
    let mut sorted: Vec<_> = chains.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (c, seq, hash) in sorted {
        t += &format!("chain {c} {seq} {hash}\n");
    }
    t
}

fn ed25519(public: &[u8], msg: &[u8], sig: &[u8]) -> bool {
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public).verify(msg, sig).is_ok()
}

/// An anchor from a file: the JSON itself, or a saved mail with the anchor block in it.
pub fn parse_anchor(text: &str) -> Result<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        return Ok(v);
    }
    let inner = match (text.find(BEGIN), text.find(END)) {
        (Some(b), Some(e)) if e > b => &text[b + BEGIN.len()..e],
        _ => text,
    };
    let (Some(b), Some(e)) = (inner.find('{'), inner.rfind('}')) else { bail!("no audit anchor in this file") };
    serde_json::from_str(&inner[b..=e]).map_err(|e| anyhow::anyhow!("the audit anchor isn't valid JSON: {e}"))
}

/// An anchor's chains as (chain, seq, hash).
fn anchor_chains(a: &Value) -> Vec<(String, i64, String)> {
    a["chains"].as_array().map(|c| c.iter().map(|c| (c["chain"].as_str().unwrap_or("").to_string(), c["seq"].as_i64().unwrap_or(0), c["hash"].as_str().unwrap_or("").to_string())).collect()).unwrap_or_default()
}

/// Checks an anchor's own signature; returns its (key id, public key).
pub fn check_anchor(a: &Value) -> Result<(String, Vec<u8>)> {
    if a["type"] != "hitop-audit-anchor" {
        bail!("not a HITOP audit anchor");
    }
    let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
    let public = super::client::unb64(&s("publicKey"))?;
    if key_id(&public) != s("keyId") {
        bail!("the anchor's key id doesn't belong to its public key");
    }
    let text = anchor_text(&s("server"), &s("org"), &s("day"), a["at"].as_i64().unwrap_or(0), &anchor_chains(a));
    if !ed25519(&public, text.as_bytes(), &super::client::unb64(&s("signature"))?) {
        bail!("the anchor of {} isn't signed by the key it names (changed after the server made it)", s("day"));
    }
    Ok((s("keyId"), public))
}

/// What a verified export holds.
#[derive(Debug, Default)]
pub struct Export {
    pub server: String,
    pub org: String,
    pub key_id: String,
    pub public: Vec<u8>,
    /// chain → (head seq, head hash)
    pub heads: BTreeMap<String, (i64, String)>,
    /// chain → seq the retention purge removed up to
    pub purged: BTreeMap<String, i64>,
    /// (chain, seq) → hash of every row
    pub rows: BTreeMap<(String, i64), String>,
}

/// Checks an export: the signature line last (SHA-256 of every byte before it, Ed25519 by the
/// key the header names), then every chain recomputed from where the purge left it to its head.
pub fn verify_export(bytes: &[u8]) -> Result<Export> {
    let text = std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("the export isn't UTF-8"))?;
    let trimmed = text.trim_end_matches('\n');
    let cut = trimmed.rfind('\n').map(|i| i + 1).ok_or_else(|| anyhow::anyhow!("the export has no signature line"))?;
    let sig: Value = serde_json::from_str(trimmed[cut..].trim()).map_err(|_| anyhow::anyhow!("the export's last line isn't its signature"))?;
    let header: Value = serde_json::from_str(text.lines().next().unwrap_or("")).unwrap_or(Value::Null);
    if header["type"] != "hitop-audit-export" || sig["type"] != "signature" {
        bail!("not a HITOP audit export");
    }
    let mut x = Export { server: header["server"].as_str().unwrap_or("").into(), org: header["org"].as_str().unwrap_or("").into(), key_id: header["keyId"].as_str().unwrap_or("").into(), ..Default::default() };
    x.public = super::client::unb64(header["publicKey"].as_str().unwrap_or(""))?;
    if key_id(&x.public) != x.key_id || sig["keyId"].as_str() != Some(x.key_id.as_str()) {
        bail!("the export's key id doesn't belong to its public key");
    }
    let digest = Sha256::digest(&bytes[..cut]);
    if sig["sha256"].as_str() != Some(hex(&digest).as_str()) {
        bail!("the export was changed after the server signed it (its SHA-256 differs)");
    }
    if !ed25519(&x.public, &[EXPORT_LABEL, digest.as_slice()].concat(), &super::client::unb64(sig["signature"].as_str().unwrap_or(""))?) {
        bail!("the export's signature doesn't verify");
    }
    let mut walk: BTreeMap<String, (i64, String)> = BTreeMap::new();
    for line in text[..cut].lines().skip(1) {
        let v: Value = serde_json::from_str(line).map_err(|_| anyhow::anyhow!("a line of the export isn't JSON"))?;
        let chain = v["chain"].as_str().unwrap_or("").to_string();
        match v["type"].as_str() {
            Some("chain") => {
                x.heads.insert(chain.clone(), (v["head"]["seq"].as_i64().unwrap_or(0), v["head"]["hash"].as_str().unwrap_or("").into()));
                let p = v["purgedThrough"]["seq"].as_i64().unwrap_or(0);
                x.purged.insert(chain.clone(), p);
                walk.insert(chain, (p, v["purgedThrough"]["hash"].as_str().unwrap_or(GENESIS).into()));
            }
            Some("event") => {
                let Some((seq, prev)) = walk.get(&chain).cloned() else { bail!("an event of a chain the export doesn't list ({chain})") };
                let n = v["seq"].as_i64().unwrap_or(0);
                if n != seq + 1 {
                    bail!("audit log {chain}: row {} is missing", seq + 1);
                }
                if v["prevHash"].as_str() != Some(prev.as_str()) {
                    bail!("audit log {chain}: row {n} doesn't follow the row before it");
                }
                let hash = row_hash(&prev, &chain, &v);
                if v["hash"].as_str() != Some(hash.as_str()) {
                    bail!("audit log {chain}: row {n} was changed (its hash doesn't match its contents)");
                }
                x.rows.insert((chain.clone(), n), hash.clone());
                walk.insert(chain, (n, hash));
            }
            other => bail!("unknown line in the export: {other:?}"),
        }
    }
    for (chain, head) in &x.heads {
        if walk.get(chain) != Some(head) {
            bail!("audit log {chain}: the chain doesn't end at its head ({})", head.0);
        }
    }
    Ok(x)
}

/// Checks that every head `anchor` names is in `x` at that seq with that hash. Returns notes
/// (chains purged past the anchor, which can't be checked any more).
pub fn match_anchor(x: &Export, anchor: &Value) -> Result<Vec<String>> {
    let (kid, public) = check_anchor(anchor)?;
    let day = anchor["day"].as_str().unwrap_or("?");
    if kid != x.key_id || public != x.public {
        bail!("the anchor of {day} was signed by another key ({kid}) than the export ({})", x.key_id);
    }
    if anchor["org"].as_str() != Some(x.org.as_str()) {
        bail!("the anchor of {day} is for another organization ({})", anchor["org"].as_str().unwrap_or("?"));
    }
    let mut notes = Vec::new();
    for (chain, seq, hash) in anchor_chains(anchor) {
        if seq == 0 {
            continue;
        }
        let Some((head, _)) = x.heads.get(&chain) else { bail!("audit log {chain}: anchored on {day} but missing from the export") };
        if seq > *head {
            bail!("the export is older than the anchor of {day} ({chain} row {seq}; the export ends at {head}): export again");
        }
        if seq <= x.purged.get(&chain).copied().unwrap_or(0) {
            notes.push(format!("{chain}: row {seq} of {day} is past the retention purge, can't check"));
            continue;
        }
        if x.rows.get(&(chain.clone(), seq)) != Some(&hash) {
            bail!("audit log {chain}: row {seq} isn't the one anchored on {day}: the log was rewritten after {day}");
        }
    }
    Ok(notes)
}

/// The server's audit key, trusted on first use per server; a different one is refused.
fn pin_key(server: &str, x: &Export) -> Result<()> {
    let server = server.trim_end_matches('/').to_string();
    let mut keys = store::load_audit_keys()?;
    let public = super::client::b64(&x.public);
    match keys.get(&server) {
        Some((kid, pk)) if *pk != public => bail!("{} the audit signing key of {server} changed (was {kid}, now {}); if the operator rotated it on purpose, remove the entry from audit-keys.json", "SECURITY:".red().bold(), x.key_id),
        Some(_) => Ok(()),
        None => {
            keys.insert(server, (x.key_id.clone(), public));
            store::save_audit_keys(&keys)
        }
    }
}

fn summary(x: &Export, anchors: usize, notes: &[String]) {
    for n in notes {
        eprintln!("{} {n}", "note:".yellow());
    }
    let rows = x.rows.len();
    let with = if anchors == 0 { String::new() } else { format!(", {anchors} anchor(s) matched") };
    println!("{} audit export of {} intact: {} chains, {rows} rows checked here, signed by {}{with}", "✓".green(), x.org, x.heads.len(), x.key_id);
}

fn load_anchors(files: &[std::path::PathBuf]) -> Result<Vec<Value>> {
    files.iter().map(|f| parse_anchor(&std::fs::read_to_string(f).map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?)).collect()
}

fn check_all(server: Option<&str>, bytes: &[u8], anchors: &[Value]) -> Result<Export> {
    let x = verify_export(bytes)?;
    let mut notes = Vec::new();
    for a in anchors {
        notes.extend(match_anchor(&x, a)?);
    }
    pin_key(server.unwrap_or(&x.server), &x)?;
    summary(&x, anchors.len(), &notes);
    Ok(x)
}

/// `hitop sync audit --export --org <org> [--out <file>] [--anchor <file>]...`.
pub fn export(api: &Api, server: &str, org: &str, org_name: &str, out: Option<std::path::PathBuf>, anchor_files: &[std::path::PathBuf]) -> Result<()> {
    let anchors = load_anchors(anchor_files)?;
    let bytes = api.bytes(&format!("/v1/orgs/{org}/audit/export"))?;
    let day = time(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0));
    let safe: String = org_name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' }).collect();
    let out = out.unwrap_or_else(|| std::path::PathBuf::from(format!("hitop-audit-{safe}-{}.jsonl", &day[..10])));
    std::fs::write(&out, &bytes).map_err(|e| anyhow::anyhow!("{}: {e}", out.display()))?;
    eprintln!("Saved {} ({} bytes)", out.display(), bytes.len());
    check_all(Some(server), &bytes, &anchors).map(|_| ())
}

/// `hitop sync audit --verify --file <export> [--anchor <file>]...` (offline), or with only
/// `--anchor`: the export of the anchors' org fetched from the server (`api`).
pub fn verify_org(api: Option<(&Api, &str)>, file: Option<&std::path::Path>, anchor_files: &[std::path::PathBuf]) -> Result<()> {
    let anchors = load_anchors(anchor_files)?;
    match file {
        Some(f) => {
            let bytes = std::fs::read(f).map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
            check_all(None, &bytes, &anchors).map(|_| ())
        }
        None => {
            let Some((api, server)) = api else { bail!("sign in, or give the export with --file") };
            let org = anchors.first().and_then(|a| a["org"].as_str()).ok_or_else(|| anyhow::anyhow!("--verify --anchor needs an anchor file (or --file <export>)"))?.to_string();
            if anchors.iter().any(|a| a["org"].as_str() != Some(org.as_str())) {
                bail!("the anchors are of different organizations: check them one org at a time");
            }
            let bytes = api.bytes(&format!("/v1/orgs/{org}/audit/export"))?;
            check_all(Some(server), &bytes, &anchors).map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anchor_text_matches_the_server() {
        // The vector pinned in sync-server/src/anchors.rs (`anchor_text_is_pinned`).
        let t = anchor_text("https://sync.example/", "o1", "2026-11-03", 1_793_700_000, &[("ws2".into(), 3, "bb".into()), ("org:o1".into(), 2, "aa".into())]);
        assert_eq!(t, "hitop-sync/v1/audit-anchor\nserver https://sync.example/\norg o1\nday 2026-11-03\nat 1793700000\nchain org:o1 2 aa\nchain ws2 3 bb\n");
    }

    #[test]
    fn anchors_are_read_from_json_or_a_saved_mail() {
        let a = json!({ "type": "hitop-audit-anchor", "org": "o1", "chains": [] });
        assert_eq!(parse_anchor(&a.to_string()).unwrap(), a);
        let mail = format!("Subject: HITOP: audit anchor\n\nKeep this mail {{really}}.\n\n{BEGIN}\n{}\n{END}\n\nOwners can turn it off {{here}}.\n", serde_json::to_string_pretty(&a).unwrap());
        assert_eq!(parse_anchor(&mail).unwrap(), a);
        assert!(parse_anchor("nothing here").is_err());
    }

    #[test]
    fn a_signed_anchor_verifies_and_a_changed_one_does_not() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pair = Ed25519KeyPair::from_seed_unchecked(&[9u8; 32]).unwrap();
        let public = pair.public_key().as_ref().to_vec();
        let chains = vec![("org:o1".to_string(), 2, "aa".to_string())];
        let sig = pair.sign(anchor_text("https://s/", "o1", "2026-11-03", 5, &chains).as_bytes());
        let mut a = json!({ "type": "hitop-audit-anchor", "server": "https://s/", "org": "o1", "day": "2026-11-03", "at": 5, "chains": [{ "chain": "org:o1", "seq": 2, "hash": "aa" }], "keyId": key_id(&public), "publicKey": super::super::client::b64(&public), "signature": super::super::client::b64(sig.as_ref()) });
        assert_eq!(check_anchor(&a).unwrap().0, key_id(&public));
        a["chains"][0]["seq"] = json!(1);
        assert!(check_anchor(&a).is_err());
    }

    #[test]
    fn the_row_hash_matches_the_server() {
        // The vector pinned in sync-server/src/audit.rs (`the_hash_is_pinned`).
        let row = json!({ "seq": 1, "at": 1_700_000_000, "event": "signin", "actor": { "id": "u1", "email": "a@example.com" }, "target": "", "detail": { "method": "email" }, "detailJson": "{\"method\":\"email\"}", "ip": "203.0.113.0", "userAgent": "hitop/1.0" });
        assert_eq!(row_hash(GENESIS, "", &row), "af2334f0d9bdfd192619c1a7f10ddb9b3fbc8c6ab807a1c81eac9c5d66dbe170");
    }
}
