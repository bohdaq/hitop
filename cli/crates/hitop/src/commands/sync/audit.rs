//! `hitop sync audit`: a workspace's audit log (owners), or your own account's (`--mine`).
//! `--verify` recomputes the hash chain here, from the rows the server sends, instead of taking
//! the server's word for it, and remembers the head it saw (`audit-heads.json`): a log rewritten
//! later from that row on is caught the next time. The row hash is the server's
//! (sync-server/src/audit.rs, THREAT_MODEL.md "Audit log").

use anyhow::{bail, Result};
use colored::Colorize;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_row_hash_matches_the_server() {
        // The vector pinned in sync-server/src/audit.rs (`the_hash_is_pinned`).
        let row = json!({ "seq": 1, "at": 1_700_000_000, "event": "signin", "actor": { "id": "u1", "email": "a@example.com" }, "target": "", "detail": { "method": "email" }, "detailJson": "{\"method\":\"email\"}", "ip": "203.0.113.0", "userAgent": "hitop/1.0" });
        assert_eq!(row_hash(GENESIS, "", &row), "af2334f0d9bdfd192619c1a7f10ddb9b3fbc8c6ab807a1c81eac9c5d66dbe170");
    }
}
