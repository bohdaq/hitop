//! Signed audit export and daily anchors (HITOP Cloud C5, anchors.rs; THREAT_MODEL item 7). The
//! test checks the export and the anchors the way `hitop sync audit --verify --anchor` does: the
//! signatures with the server's public key, every chain recomputed, each anchored head found in
//! the export; and that a chain rewritten after an anchor was mailed no longer matches it.

mod common;

use std::collections::BTreeMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use common::*;
use hitop_sync_server::anchors;
use hitop_sync_server::audit::{row_hash, GENESIS};
use ring::signature::{UnparsedPublicKey, ED25519};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const WS: &str = "1f0e0d0c0b0a09080706050403020100";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Checks an export as a client would; `(chain → (head seq, head hash))` or the first problem.
fn verify_export(
    bytes: &[u8],
    public_key: &[u8],
) -> Result<BTreeMap<String, (i64, String)>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8")?;
    let body_len = text
        .trim_end_matches('\n')
        .rfind('\n')
        .ok_or("no signature line")?
        + 1;
    let sig: Value =
        serde_json::from_str(text[body_len..].trim()).map_err(|_| "bad signature line")?;
    if sig["type"] != "signature" {
        return Err("the last line isn't the signature".into());
    }
    let digest = Sha256::digest(&bytes[..body_len]);
    if sig["sha256"].as_str() != Some(hex(&digest).as_str()) {
        return Err("the file was changed (sha256)".into());
    }
    let s = URL_SAFE_NO_PAD
        .decode(sig["signature"].as_str().unwrap())
        .unwrap();
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(&[anchors::EXPORT_LABEL, digest.as_slice()].concat(), &s)
        .map_err(|_| "bad signature")?;
    let mut heads = BTreeMap::new();
    let mut walk: BTreeMap<String, (i64, String)> = BTreeMap::new();
    for line in text[..body_len].lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        match v["type"].as_str().unwrap() {
            "hitop-audit-export" => {}
            "chain" => {
                let c = v["chain"].as_str().unwrap().to_string();
                heads.insert(
                    c.clone(),
                    (
                        v["head"]["seq"].as_i64().unwrap(),
                        v["head"]["hash"].as_str().unwrap().to_string(),
                    ),
                );
                walk.insert(
                    c,
                    (
                        v["purgedThrough"]["seq"].as_i64().unwrap(),
                        v["purgedThrough"]["hash"].as_str().unwrap().to_string(),
                    ),
                );
            }
            "event" => {
                let c = v["chain"].as_str().unwrap();
                let (seq, prev) = walk.get(c).cloned().ok_or("an event of an unknown chain")?;
                let n = v["seq"].as_i64().unwrap();
                if n != seq + 1 || v["prevHash"].as_str() != Some(prev.as_str()) {
                    return Err(format!("{c}: row {n} doesn't follow"));
                }
                let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                let h = row_hash(
                    &prev,
                    c,
                    n,
                    v["at"].as_i64().unwrap(),
                    &s("event"),
                    v["actor"]["id"].as_str().unwrap(),
                    v["actor"]["email"].as_str().unwrap(),
                    &s("target"),
                    &s("detailJson"),
                    &s("ip"),
                    &s("userAgent"),
                );
                if Some(h.as_str()) != v["hash"].as_str() {
                    return Err(format!("{c}: row {n} was changed"));
                }
                walk.insert(c.to_string(), (n, h));
            }
            other => return Err(format!("unknown line {other}")),
        }
    }
    if walk != heads {
        return Err("a chain doesn't end at its head".into());
    }
    Ok(heads)
}

fn verify_anchor(a: &Value) -> Result<(), String> {
    let chains: Vec<(String, i64, String)> = a["chains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["chain"].as_str().unwrap().into(),
                c["seq"].as_i64().unwrap(),
                c["hash"].as_str().unwrap().into(),
            )
        })
        .collect();
    let text = anchors::anchor_text(
        a["server"].as_str().unwrap(),
        a["org"].as_str().unwrap(),
        a["day"].as_str().unwrap(),
        a["at"].as_i64().unwrap(),
        &chains,
    );
    let key = URL_SAFE_NO_PAD
        .decode(a["publicKey"].as_str().unwrap())
        .unwrap();
    let sig = URL_SAFE_NO_PAD
        .decode(a["signature"].as_str().unwrap())
        .unwrap();
    UnparsedPublicKey::new(&ED25519, key)
        .verify(text.as_bytes(), &sig)
        .map_err(|_| "bad anchor signature".to_string())
}

/// Every anchored head must be in the export at that seq with that hash.
fn matches(anchor: &Value, export: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(export).unwrap();
    for c in anchor["chains"].as_array().unwrap() {
        let (chain, seq, hash) = (
            c["chain"].as_str().unwrap(),
            c["seq"].as_i64().unwrap(),
            c["hash"].as_str().unwrap(),
        );
        if seq == 0 {
            continue;
        }
        let found = text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["type"] == "event" && v["chain"] == chain && v["seq"] == seq);
        if found.as_ref().and_then(|v| v["hash"].as_str()) != Some(hash) {
            return Err(format!(
                "{chain}: row {seq} isn't the one anchored: the log was rewritten"
            ));
        }
    }
    Ok(())
}

fn export(s: &Server, d: &Device, org: &str) -> Reply {
    s.call(
        "GET",
        &format!("/v1/orgs/{org}/audit/export"),
        Some(&d.session),
        None,
    )
}

fn mailed_anchor(s: &Server, to: &str) -> Value {
    let outbox = s.outbox.lock().unwrap();
    let mail = outbox
        .iter()
        .rev()
        .find(|m| m.to == to && m.subject.contains("audit anchor"))
        .expect("an anchor mail");
    let start = mail
        .body
        .find("-----BEGIN HITOP AUDIT ANCHOR-----")
        .unwrap()
        + 35;
    let end = mail.body.find("-----END HITOP AUDIT ANCHOR-----").unwrap();
    serde_json::from_str(mail.body[start..end].trim()).unwrap()
}

fn anchor_mails(s: &Server) -> usize {
    s.outbox
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.subject.contains("audit anchor"))
        .count()
}

#[test]
fn the_export_and_the_daily_anchor_verify_and_a_rewrite_after_the_anchor_shows() {
    let s = Server::new("audit-export");
    let ada = Device::new(&s, "ada@corp.test");
    let bob = Device::new(&s, "bob@corp.test");
    let org = s
        .call(
            "POST",
            "/v1/orgs",
            Some(&ada.session),
            Some(json!({ "name": "Corp" })),
        )
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let k = create_workspace(&s, &ada, WS);
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{org}/workspaces"),
            Some(&ada.session),
            Some(json!({ "workspace": WS }))
        )
        .status,
        200
    );
    let wraps = wraps_for(
        &[(bob.key_id.clone(), bob.keys.public_key().to_vec())],
        WS,
        1,
        &k,
    );
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/workspaces/{WS}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@corp.test", "role": "editor", "wraps": wraps }))
        )
        .status,
        201
    );
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{org}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@corp.test", "role": "member" }))
        )
        .status,
        201
    );

    let key = s.call("GET", "/v1/audit/key", None, None).json;
    assert_eq!(key["alg"], "Ed25519");
    let public = URL_SAFE_NO_PAD
        .decode(key["publicKey"].as_str().unwrap())
        .unwrap();

    // Owners and admins export; members don't.
    assert_eq!(export(&s, &bob, &org).status, 403);
    let r = export(&s, &ada, &org);
    assert_eq!(r.status, 200);
    assert!(r.header("content-disposition").unwrap().contains(".jsonl"));
    let heads = verify_export(&r.raw, &public).unwrap();
    assert_eq!(
        heads.keys().cloned().collect::<Vec<_>>(),
        vec![WS.to_string(), format!("org:{org}")]
    );
    assert!(
        heads[WS].0 >= 2 && heads[&format!("org:{org}")].0 >= 3,
        "{heads:?}"
    );
    // A changed byte breaks it.
    let mut bad = r.raw.clone();
    let i = bad.iter().position(|b| *b == b'b').unwrap();
    bad[i] = b'c';
    assert!(verify_export(&bad, &public).is_err());

    // The daily anchor: made once a day, mailed to the owners, and it verifies.
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 1);
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 0, "once a day");
    let anchor = mailed_anchor(&s, "ada@corp.test");
    assert!(
        s.outbox
            .lock()
            .unwrap()
            .iter()
            .all(|m| !(m.to == "bob@corp.test" && m.subject.contains("anchor"))),
        "owners only"
    );
    verify_anchor(&anchor).unwrap();
    assert_eq!(anchor["org"], org.as_str());
    assert_eq!(anchor["keyId"], key["keyId"]);
    let latest = export(&s, &ada, &org).raw;
    matches(&anchor, &latest).unwrap();
    // A tampered anchor doesn't verify.
    let mut forged = anchor.clone();
    forged["chains"][1]["seq"] = json!(1);
    assert!(verify_anchor(&forged).is_err());

    // Exports are events too (on the org's chain), so the next day's heads moved: mailed. A day
    // with no change after it: an anchor is kept, no mail.
    s.advance(86_400);
    let mails = anchor_mails(&s);
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 1);
    assert_eq!(anchor_mails(&s), mails + 1);
    s.advance(86_400);
    let mails = anchor_mails(&s);
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 1);
    assert_eq!(anchor_mails(&s), mails, "nothing moved, nothing mailed");
    // A day with changes: mailed.
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/workspaces/{WS}/members/{}", bob.user_id),
            Some(&ada.session),
            Some(json!({ "role": "viewer" }))
        )
        .status,
        200
    );
    s.advance(86_400);
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 1);
    assert_eq!(anchor_mails(&s), mails + 1);
    let listed = s
        .call(
            "GET",
            &format!("/v1/orgs/{org}/audit/anchors"),
            Some(&ada.session),
            None,
        )
        .json;
    assert_eq!(listed["anchors"].as_array().unwrap().len(), 4);
    assert_eq!(
        (
            listed["mailAnchors"].as_bool(),
            listed["publicAnchors"].as_bool()
        ),
        (Some(true), Some(false))
    );

    // Someone with the database rewrites the workspace chain from row 1 (fresh hashes, new head).
    // The export still verifies by itself, but no longer matches the anchor that was mailed.
    let state = s.app.state();
    state
        .db
        .tx(|tx| {
            let rows = tx.query("SELECT seq, at, event, actor_id, actor_email, target, detail, ip, user_agent FROM audit_log WHERE workspace_id = $1 ORDER BY seq", &[WS.into()])?;
            let mut prev = GENESIS.to_string();
            for r in rows {
                let target = if r.text(2) == "member_added" { "someone-else".to_string() } else { r.text(5) };
                let h = row_hash(&prev, WS, r.int(0), r.int(1), &r.text(2), &r.text(3), &r.text(4), &target, &r.text(6), &r.text(7), &r.text(8));
                tx.execute("UPDATE audit_log SET target = $1, prev_hash = $2, hash = $3 WHERE workspace_id = $4 AND seq = $5", &[target.into(), prev.clone().into(), h.clone().into(), WS.into(), r.int(0).into()])?;
                tx.execute("UPDATE audit_chain SET head_hash = $1 WHERE workspace_id = $2 AND head_seq = $3", &[h.clone().into(), WS.into(), r.int(0).into()])?;
                prev = h;
            }
            Ok::<_, hitop_sync_server::db::DbError>(())
        })
        .unwrap();
    let rewritten = export(&s, &ada, &org).raw;
    verify_export(&rewritten, &public).unwrap();
    assert!(matches(&anchor, &rewritten)
        .unwrap_err()
        .contains("rewritten"));

    // Public anchors: opt-in, owners only.
    let day = anchor["day"].as_str().unwrap();
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/audit-anchors/{org}/{day}.json"),
            None,
            None
        )
        .status,
        404
    );
    let settings = json!({ "publicAnchors": true });
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/audit/settings"),
            Some(&bob.session),
            Some(settings.clone())
        )
        .status,
        403
    );
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/audit/settings"),
            Some(&ada.session),
            Some(settings)
        )
        .status,
        200
    );
    let public_anchor = s.call(
        "GET",
        &format!("/v1/audit-anchors/{org}/{day}.json"),
        None,
        None,
    );
    assert_eq!(public_anchor.status, 200);
    assert_eq!(public_anchor.json, anchor);
    // An anchor of right now, to keep next to an export.
    let now = s
        .call(
            "POST",
            &format!("/v1/orgs/{org}/audit/anchors"),
            Some(&ada.session),
            None,
        )
        .json;
    verify_anchor(&now).unwrap();
    matches(&now, &export(&s, &ada, &org).raw).unwrap();

    let events: Vec<String> = audit_rows(&s, &format!("org:{org}"))
        .iter()
        .map(|r| r["event"].as_str().unwrap().to_string())
        .collect();
    assert!(
        events.contains(&"audit_exported".to_string())
            && events.contains(&"audit_settings_changed".to_string()),
        "{events:?}"
    );
}

#[test]
fn export_and_anchors_are_business_on_hitop_cloud() {
    let s = Server::with("audit-export-plans", |c| c.plan_limits = true);
    let ada = Device::new(&s, "ada@corp.test");
    let org = s
        .call(
            "POST",
            "/v1/orgs",
            Some(&ada.session),
            Some(json!({ "name": "Corp" })),
        )
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = export(&s, &ada, &org);
    assert_eq!(
        (r.status, r.json["limit"].as_str()),
        (402, Some("auditExport"))
    );
    assert_eq!(
        anchors::daily(s.app.state()).unwrap(),
        0,
        "no anchors on Free"
    );
    hitop_sync_server::limits::set_plan(s.app.state(), &org, "team", 3).unwrap();
    assert_eq!(export(&s, &ada, &org).status, 402);
    hitop_sync_server::limits::set_plan(s.app.state(), &org, "business", 3).unwrap();
    assert_eq!(export(&s, &ada, &org).status, 200);
    assert_eq!(anchors::daily(s.app.state()).unwrap(), 1);
    // Owners can turn the mail off.
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/audit/settings"),
            Some(&ada.session),
            Some(json!({ "mailAnchors": false }))
        )
        .status,
        200
    );
    let personal = format!("personal-{}", ada.user_id);
    assert_eq!(export(&s, &ada, &personal).status, 400);
}
