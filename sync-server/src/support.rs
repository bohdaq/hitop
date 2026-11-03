//! `hitop-sync-server support-bundle` (HITOP Cloud C6, the enterprise self-host package): one JSON
//! file an operator reads, then sends to support. It holds versions, the schema, the settings with
//! every secret redacted, row counts per table, health counts and recent error lines from the
//! server's log with addresses, ids, tokens and IPs scrubbed. Never customer data: no addresses, no
//! workspace or org ids or names, no blob, run, key or audit content, no secret or token values.
//!
//! What is shown of a setting is an allowlist by name: everything else is only "(set)". URLs that
//! can carry credentials (`HITOP_SYNC_DATABASE`, `SMTP_URL`) are cut to scheme, host and port.

use std::collections::BTreeMap;
use std::io::BufRead;

use serde_json::{json, Value as Json};

use crate::db::{Db, DbError};
use crate::State;

/// Settings whose values are shown (operator configuration, never a credential).
const SHOWN: &[&str] = &[
    "HITOP_SYNC_PUBLIC_URL",
    "HITOP_SYNC_RP_ID",
    "HITOP_SYNC_RP_ORIGINS",
    "HITOP_SYNC_ALLOWED_ORIGINS",
    "HITOP_SYNC_MAX_BLOB_BYTES",
    "HITOP_SYNC_MAX_WORKSPACE_BYTES",
    "HITOP_SYNC_MAX_WORKSPACES",
    "HITOP_SYNC_MAX_JSON_BYTES",
    "HITOP_SYNC_MAX_RUN_BYTES",
    "HITOP_SYNC_TRUSTED_PROXIES",
    "HITOP_SYNC_AUDIT_IP",
    "HITOP_SYNC_AUDIT_RETENTION_DAYS",
    "HITOP_SYNC_AUDIT_ANCHORS",
    "HITOP_SYNC_LIMITS",
    "HITOP_SYNC_PORTAL_URL",
    "HITOP_SYNC_RATE_LIMITS",
    "HITOP_SYNC_RATE_AUTH",
    "HITOP_SYNC_RATE_WRITE",
    "HITOP_SYNC_RATE_WRITE_IP",
    "HITOP_SYNC_RATE_CREATE",
    "HITOP_SYNC_PASSWORDS",
    "HITOP_SYNC_TRIAL_DAYS",
    "HITOP_SYNC_RUN_RETENTION_DAYS",
    "HITOP_SYNC_ALERTS_PER_HOUR",
    "HITOP_SYNC_ALERT_WEBHOOKS",
    "HITOP_SYNC_DNS_URL",
    "HITOP_SYNC_MAIL",
    "HITOP_SYNC_STRIPE_API_URL",
    "HITOP_SYNC_GITHUB_URL",
    "HITOP_SYNC_GITHUB_API_URL",
    "HITOP_SYNC_GOOGLE_ISSUER",
    "HITOP_SYNC_OIDC_ISSUER",
    "HITOP_SYNC_OIDC_SCOPES",
    "HITOP_SYNC_OIDC_NAME",
    "HITOP_SYNC_OIDC_GROUPS_CLAIM",
    "HITOP_SYNC_OIDC_REQUIRED_GROUPS",
    "HITOP_SYNC_OIDC_ALLOWED_DOMAINS",
    "HITOP_SYNC_OIDC_ASSUME_VERIFIED",
    "HITOP_SYNC_SSO_REQUIRED",
    "SMTP_TLS_SERVER_NAME",
    "RWS_CONFIG_IP",
    "RWS_CONFIG_PORT",
    "RWS_CONFIG_LOG_FORMAT",
    "RWS_CONFIG_MAX_BODY_SIZE_IN_BYTES",
];

/// Parts of a name that make it a secret even if a pattern above would show it.
const SECRET_WORDS: &[&str] = &["SECRET", "TOKEN", "KEY", "PASS", "PEPPER", "CREDENTIAL", "PRIVATE", "CERT"];

pub struct Options {
    /// A log file to take recent error lines from (instead of the journal).
    pub log: Option<std::path::PathBuf>,
    /// The systemd unit whose journal is read when there is no `log` (`hitop-sync`).
    pub unit: String,
    pub since_hours: u32,
    pub journal: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { log: None, unit: "hitop-sync".into(), since_hours: 72, journal: true }
    }
}

/// What a setting shows in the bundle.
pub fn shown_value(name: &str, value: &str) -> String {
    let secret = SECRET_WORDS.iter().any(|w| name.contains(w));
    if name == "HITOP_SYNC_DATABASE" {
        return database_shape(value);
    }
    if name == "SMTP_URL" {
        return url_shape(value);
    }
    if name == "SMTP_FROM" {
        // The sender address: only its domain.
        return value.rsplit_once('@').map_or_else(|| "(set)".into(), |(_, d)| format!("<address>@{}", d.trim_end_matches('>')));
    }
    let listed = SHOWN.contains(&name) || name.starts_with("HITOP_SYNC_STRIPE_PRICE_") || (name.starts_with("RWS_CONFIG_") && !secret);
    if listed && !secret {
        scrub_line(value)
    } else {
        "(set)".into()
    }
}

fn url_shape(value: &str) -> String {
    match url::Url::parse(value) {
        Ok(u) => {
            let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
            format!("{}://{}{port}", u.scheme(), u.host_str().unwrap_or(""))
        }
        Err(_) => "(set, not a URL)".into(),
    }
}

fn database_shape(value: &str) -> String {
    if let Some(path) = value.strip_prefix("sqlite:") {
        return format!("sqlite:{path}");
    }
    if value.starts_with("postgres") {
        // Host or socket only: never the user, password or options.
        return match url::Url::parse(value) {
            Ok(u) => {
                let host = u.host_str().filter(|h| !h.is_empty()).map(String::from).or_else(|| u.query_pairs().find(|(k, _)| k == "host").map(|(_, v)| v.into_owned())).unwrap_or_else(|| "(default)".into());
                let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
                format!("postgres://{host}{port}/{}", u.path().trim_start_matches('/'))
            }
            Err(_) => "postgres (set)".into(),
        };
    }
    "(set)".into()
}

/// Replaces what could identify a person or a secret in a log line: addresses, long hex ids,
/// token-like strings, IP addresses, URL queries.
pub fn scrub_line(line: &str) -> String {
    let mut out = Vec::new();
    for word in line.split(' ') {
        out.push(scrub_word(word));
    }
    let s = out.join(" ");
    s.chars().take(300).collect()
}

fn scrub_word(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    if word.contains('@') && word.contains('.') {
        return "<email>".into();
    }
    if let Some((before, _)) = word.split_once('?').filter(|(b, _)| b.contains("://")) {
        return format!("{}?<query>", scrub_word(before));
    }
    if word.contains("://") {
        // A URL: keep scheme and host, scrub its path segments.
        if let Ok(u) = url::Url::parse(word.trim_end_matches([',', '.', ')', ';'])) {
            let path: Vec<String> = u.path().split('/').map(scrub_token).collect();
            let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
            return format!("{}://{}{port}{}", u.scheme(), u.host_str().unwrap_or(""), path.join("/"));
        }
    }
    // Keep punctuation around a token: `(abc...)`, `id=...,`.
    let mut rebuilt = String::new();
    let mut current = String::new();
    for c in word.chars() {
        if "=:/(),\"'[]".contains(c) {
            rebuilt.push_str(&scrub_token(&current));
            current.clear();
            rebuilt.push(c);
        } else {
            current.push(c);
        }
    }
    rebuilt.push_str(&scrub_token(&current));
    rebuilt
}

fn scrub_token(t: &str) -> String {
    let core = t.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
    if core.is_empty() {
        return t.to_string();
    }
    let is_ipv4 = core.split('.').count() == 4 && core.split('.').all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()));
    if is_ipv4 {
        return t.replace(core, "<ip>");
    }
    let hexish = core.len() >= 12 && core.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-');
    if hexish && core.bytes().any(|b| b.is_ascii_digit()) {
        return t.replace(core, "<id>");
    }
    let tokenish = core.len() >= 20 && core.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && core.bytes().any(|b| b.is_ascii_digit()) && core.bytes().any(|b| b.is_ascii_alphabetic());
    if tokenish {
        return t.replace(core, "<token>");
    }
    t.to_string()
}

/// A line worth sending: an error, a failure, a broken chain, a panic. Mail bodies (the `log`
/// mailer prints them) never.
fn is_error(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    if l.starts_with("mail to ") {
        return false;
    }
    ["error", "failed", "fail:", "refused", "broken", "panicked", "panic", "database", "timed out", "timeout", " 500", " 502", " 503"].iter().any(|w| l.contains(w))
}

fn recent_errors(opts: &Options) -> Json {
    let mut lines: Vec<String> = Vec::new();
    let source;
    if let Some(path) = &opts.log {
        source = format!("file {}", path.display());
        match std::fs::File::open(path) {
            Ok(f) => {
                for l in std::io::BufReader::new(f).lines().map_while(Result::ok) {
                    lines.push(l);
                }
            }
            Err(e) => return json!({ "source": source, "lines": [], "note": format!("can't read the log: {e}") }),
        }
    } else if opts.journal {
        source = format!("journalctl -u {} (last {} h)", opts.unit, opts.since_hours);
        let out = std::process::Command::new("journalctl").args(["-u", &opts.unit, "--since", &format!("-{}h", opts.since_hours), "-o", "cat", "--no-pager", "-n", "5000"]).output();
        match out {
            Ok(o) if o.status.success() => lines = String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect(),
            Ok(o) => return json!({ "source": source, "lines": [], "note": format!("journalctl answered {}; pass --log <file> with the server's log instead", o.status) }),
            Err(_) => return json!({ "source": "none", "lines": [], "note": "no journal here (journalctl not found); pass --log <file> with the server's stderr log" }),
        }
    } else {
        return json!({ "source": "none", "lines": [], "note": "not collected (--no-journal)" });
    }
    let errors: Vec<String> = lines.iter().filter(|l| is_error(l)).map(|l| scrub_line(l)).collect();
    let total = errors.len();
    let last: Vec<String> = errors.into_iter().rev().take(200).rev().collect();
    json!({ "source": source, "matched": total, "lines": last, "note": "error lines only, addresses, ids, tokens and IPs replaced" })
}

fn count(state: &State, sql: &str, params: &[crate::db::Value]) -> Result<i64, DbError> {
    state.db.tx(|tx| Ok(tx.query_one(sql, params)?.map_or(0, |r| r.int(0))))
}

/// The bundle as JSON.
pub fn bundle(state: &State, opts: &Options, env: &[(String, String)]) -> Result<Json, DbError> {
    let stats = crate::dbtools::stats(&state.db)?;
    let kind = state.db.kind();
    let db_version = state.db.tx(|tx| {
        let sql = if kind == "postgres" { "SHOW server_version" } else { "SELECT sqlite_version()" };
        Ok::<_, DbError>(tx.query_one(sql, &[])?.map(|r| r.text(0)).unwrap_or_default())
    })?;
    let mut settings = BTreeMap::new();
    for (name, value) in env {
        if name.starts_with("HITOP_SYNC_") || name.starts_with("SMTP_") || name.starts_with("RWS_CONFIG_") {
            settings.insert(name.clone(), json!(shown_value(name, value)));
        }
    }
    let c = &state.config;
    let chains = crate::audit::verify_all(state).map_err(|e| DbError(e.to_string()))?;
    let broken = chains.iter().filter(|(_, v)| v.problem.is_some()).count();
    let week = state.now() - 7 * 86_400;
    let over = if c.plan_limits { Some(state.db.tx(|tx| crate::limits::over_limits(state, tx)).map(|o| o.len()).unwrap_or(0)) } else { None };
    let outcomes: BTreeMap<String, i64> = state.db.tx(|tx| {
        Ok::<_, DbError>(tx.query("SELECT outcome, COUNT(*) FROM billing_events WHERE received_at > $1 GROUP BY outcome", &[week.into()])?.iter().map(|r| (r.text(0), r.int(1))).collect())
    })?;
    Ok(json!({
        "bundle": "hitop-sync-server support bundle",
        "bundleVersion": 1,
        "createdAt": state.now(),
        "readBeforeSending": "This file has no customer data, ids or secrets by design. Read it before you send it; remove anything you don't want to share.",
        "versions": {
            "server": env!("CARGO_PKG_VERSION"),
            "schemaInDatabase": stats.schema,
            "schemaOfThisBinary": Db::schema_version(),
            "database": kind,
            "databaseVersion": db_version,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "features": { "postgres": cfg!(feature = "postgres") },
        },
        "config": {
            "settings": settings,
            "in effect": {
                "mail": state.mailer.enabled(),
                "sso": c.oidc.is_some(),
                "google": c.google.is_some(),
                "github": c.github.is_some(),
                "billing": c.billing.is_some(),
                "planLimits": c.plan_limits,
                "hostKey": c.host_key.is_some(),
                "dnsResolver": c.dns_url.is_some(),
                "auditAnchors": c.audit_anchors,
                "passwords": c.password.enabled,
            },
        },
        "database": {
            "kind": stats.kind,
            "schema": stats.schema,
            "rows": stats.rows(),
            "tables": stats.tables.iter().map(|t| json!({ "table": t.table, "rows": t.rows })).collect::<Vec<_>>(),
        },
        "health": {
            "auditChains": chains.len(),
            "auditChainsBroken": broken,
            "orgsOverALimit": over,
            "alertDeliveriesPending": count(state, "SELECT COUNT(*) FROM alert_deliveries WHERE outcome = 'queued'", &[])?,
            "alertDeliveriesFailed7d": count(state, "SELECT COUNT(*) FROM alert_deliveries WHERE outcome = 'failed' AND created_at > $1", &[week.into()])?,
            "billingEvents7d": outcomes,
            "workspacesWithRotationRequested": count(state, "SELECT COUNT(*) FROM workspaces WHERE rotation_requested_at > 0", &[])?,
            "scimTokens": count(state, "SELECT COUNT(*) FROM scim_tokens", &[])?,
        },
        "recentErrors": recent_errors(opts),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_redacted_by_name() {
        assert_eq!(shown_value("HITOP_SYNC_STRIPE_SECRET_KEY", "sk_x"), "(set)");
        assert_eq!(shown_value("HITOP_SYNC_HOST_KEY", "00"), "(set)");
        assert_eq!(shown_value("HITOP_SYNC_OIDC_CLIENT_SECRET_FILE", "/x"), "(set)");
        assert_eq!(shown_value("HITOP_SYNC_SOMETHING_NEW", "x"), "(set)");
        assert_eq!(shown_value("RWS_CONFIG_TLS_KEY", "/k"), "(set)");
        assert_eq!(shown_value("HITOP_SYNC_LIMITS", "plans"), "plans");
        assert_eq!(shown_value("SMTP_URL", "smtps://user:pw@mail.example.com:465"), "smtps://mail.example.com:465");
        assert_eq!(shown_value("SMTP_FROM", "HITOP <noreply@example.com>"), "<address>@example.com");
        assert_eq!(shown_value("HITOP_SYNC_DATABASE", "postgres://u:pw@db.internal:5432/hitop?sslmode=require"), "postgres://db.internal:5432/hitop");
        assert_eq!(shown_value("HITOP_SYNC_DATABASE", "postgres:///hitop?host=/run/postgresql&user=hitop&password=x"), "postgres:///run/postgresql/hitop");
        assert_eq!(shown_value("HITOP_SYNC_DATABASE", "sqlite:/var/lib/hitop-sync/sync.db"), "sqlite:/var/lib/hitop-sync/sync.db");
    }

    #[test]
    fn log_lines_are_scrubbed() {
        let l = scrub_line("hitop-sync-server: alert 0f0e0d0c0b0a0908 (webhook): failed for ada@corp.test from 10.1.2.3 token=hitop_pat_AbCdEf0123456789xyzXYZ https://hooks.example.com/T00/abc123def456ghi789jkl?x=1");
        assert!(!l.contains("0f0e0d0c0b0a0908"), "{l}");
        assert!(!l.contains("ada@corp.test"), "{l}");
        assert!(!l.contains("10.1.2.3"), "{l}");
        assert!(!l.contains("AbCdEf0123456789"), "{l}");
        assert!(!l.contains("x=1"), "{l}");
        assert!(l.contains("failed"), "{l}");
        assert!(l.contains("hooks.example.com"), "{l}");
        assert!(!is_error("mail to ada@corp.test: Sign in"));
        assert!(is_error("hitop-sync-server: audit purge: database is locked"));
    }
}
